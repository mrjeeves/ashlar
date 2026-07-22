//! The Ashlar compiler front end.
//!
//! Pipeline: walk `.ash` files -> lex -> parse -> resolve -> compose.
//! Every stage accumulates `diag::Diag`s; nothing panics on bad input.
//!
//! Module ownership (contract files are owned by the integrator):
//!   tokens, ast, diag, resolved, lib  — contracts
//!   lexer, parser, resolve, compose   — module implementors
//!   fixup, manifest, main             — CLI implementor

pub mod ast;
pub mod check;
pub mod compose;
pub mod diag;
pub mod eval;
pub mod fixup;
pub mod fmt;
pub mod http;
pub mod lexer;
pub mod manifest;
pub mod parser;
pub mod refactor;
pub mod resolve;
pub mod resolved;
pub mod tokens;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct CheckResult {
    pub program: resolved::Program,
    pub composed: BTreeMap<String, resolved::ComposedPart>,
    pub diags: Vec<diag::Diag>,
    /// Field-site index from the checker (empty when earlier errors kept
    /// the checker from running). See `check::FieldSite`.
    pub field_sites: Vec<check::FieldSite>,
}

impl CheckResult {
    pub fn has_errors(&self) -> bool {
        self.diags.iter().any(|d| d.is_error())
    }
}

/// Directories never scanned for sources: build output, VCS, the vendored
/// foreign libraries, static assets, and the repo's own test corpora.
const SKIP_DIRS: &[&str] = &[".git", "target", "foreign", "assets", "suites", "node_modules"];

/// Find every `.ash` file under `root`, sorted by relative path so that
/// everything downstream is deterministic (C2, F2).
pub fn find_ash_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if !SKIP_DIRS.contains(&name.as_ref()) && !name.starts_with('.') {
                    stack.push(path);
                }
            } else if name.ends_with(".ash") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn rel_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Check a whole project rooted at `root`.
pub fn check_project(root: &Path) -> CheckResult {
    let mut sources = Vec::new();
    for path in find_ash_files(root) {
        let rel = rel_path(root, &path);
        match fs::read_to_string(&path) {
            Ok(src) => sources.push((rel, src)),
            Err(e) => {
                // An unreadable file is a build fact worth an error, not a panic.
                sources.push((rel.clone(), String::new()));
                eprintln!("warning: could not read {}: {}", rel, e);
            }
        }
    }
    check_sources(sources)
}

/// Per-file front-end cache for incremental checking (F1): content hash
/// -> parsed AST + file-local diagnostics. The global phases (resolve,
/// compose, check) always rerun — they are cross-file by definition and
/// cheap next to parsing.
#[derive(Default)]
pub struct IncrementalCache {
    entries: BTreeMap<String, (u64, Option<resolved::FileEntry>, Vec<diag::Diag>)>,
}

fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// `check_sources` with a per-file cache: unchanged files skip the lexer
/// and parser entirely. This is the F1 path — a single-file change in a
/// large project re-parses one file and re-runs only the global phases.
pub fn check_sources_incremental(
    sources: Vec<(String, String)>,
    cache: &mut IncrementalCache,
) -> CheckResult {
    let mut diags: Vec<diag::Diag> = Vec::new();
    let mut files: Vec<resolved::FileEntry> = Vec::new();

    let live: std::collections::BTreeSet<String> =
        sources.iter().map(|(p, _)| p.clone()).collect();
    cache.entries.retain(|p, _| live.contains(p));

    for (path, src) in &sources {
        let h = fnv1a(src);
        let hit = cache
            .entries
            .get(path)
            .filter(|(ch, _, _)| *ch == h)
            .cloned();
        let (entry, file_diags) = match hit {
            Some((_, e, d)) => (e, d),
            None => {
                let (toks, mut lex_diags) = lexer::lex(path, src);
                let (file_ast, mut parse_diags) = parser::parse(path, &toks);
                lex_diags.append(&mut parse_diags);
                let entry = file_ast.map(|ast| resolved::FileEntry {
                    path: path.clone(),
                    ast,
                });
                cache
                    .entries
                    .insert(path.clone(), (h, entry.clone(), lex_diags.clone()));
                (entry, lex_diags)
            }
        };
        diags.extend(file_diags);
        if let Some(e) = entry {
            files.push(e);
        }
    }

    finish_check(files, diags)
}

/// Check in-memory sources: `(relative path, contents)`. This is the entry
/// point tests use; `check_project` is a thin wrapper over it.
pub fn check_sources(sources: Vec<(String, String)>) -> CheckResult {
    let mut diags: Vec<diag::Diag> = Vec::new();
    let mut files: Vec<resolved::FileEntry> = Vec::new();

    for (path, src) in &sources {
        let (toks, mut lex_diags) = lexer::lex(path, src);
        diags.append(&mut lex_diags);
        let (file_ast, mut parse_diags) = parser::parse(path, &toks);
        diags.append(&mut parse_diags);
        if let Some(ast) = file_ast {
            files.push(resolved::FileEntry {
                path: path.clone(),
                ast,
            });
        }
    }

    finish_check(files, diags)
}

/// The global phases shared by full and incremental checking.
fn finish_check(files: Vec<resolved::FileEntry>, mut diags: Vec<diag::Diag>) -> CheckResult {
    let (program, mut resolve_diags) = resolve::resolve(files);
    diags.append(&mut resolve_diags);

    let (composed, mut compose_diags) = compose::compose(&program);
    diags.append(&mut compose_diags);

    // Shape checking runs only on programs whose names resolved: earlier
    // errors would cascade into misleading shape diagnostics.
    let mut field_sites = Vec::new();
    if !diags.iter().any(|d| d.is_error()) {
        let (mut check_diags, sites) = check::check(&program, &composed);
        diags.append(&mut check_diags);
        field_sites = sites;
    }

    // Stable output order: file, then position, then id.
    diags.sort_by(|a, b| {
        (a.file.as_str(), a.span.start.line, a.span.start.col, a.id)
            .cmp(&(b.file.as_str(), b.span.start.line, b.span.start.col, b.id))
    });

    CheckResult {
        program,
        composed,
        diags,
        field_sites,
    }
}
