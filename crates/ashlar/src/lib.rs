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

    let (program, mut resolve_diags) = resolve::resolve(files);
    diags.append(&mut resolve_diags);

    let (composed, mut compose_diags) = compose::compose(&program);
    diags.append(&mut compose_diags);

    // Shape checking runs only on programs whose names resolved: earlier
    // errors would cascade into misleading shape diagnostics.
    if !diags.iter().any(|d| d.is_error()) {
        let mut check_diags = check::check(&program, &composed);
        diags.append(&mut check_diags);
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
    }
}
