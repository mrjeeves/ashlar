//! Shared helpers for the integration/meta test suites. Not itself a test
//! target (lives under tests/support/, so cargo does not compile it as one);
//! each test file pulls in what it needs with `mod support;`.
//!
//! Kept dependency-free (std only), matching the workspace's zero-dependency
//! policy (G1) — these helpers ship only in test binaries, never in the
//! `ashlar` library or CLI, but there is no reason to be the exception.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The workspace root, from any file under `crates/ashlar/tests/`.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Read a file that is expected to exist; panic with a specific, actionable
/// message (not a bare `unwrap`) if it does not — this is how tests "fail
/// cleanly" when a concurrently-authored file (docs, fixtures) isn't there
/// yet.
pub fn read_text(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("expected file to exist and be readable: {} ({})", path.display(), e))
}

/// Extract every ```ash fenced block from a markdown document, by scanning
/// for lines equal to "```ash" (open) and "```" (close). Blocks are returned
/// in document order, newline-joined, without the fence lines themselves.
pub fn extract_ash_blocks(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_end_matches('\r') == "```ash" {
            let mut j = i + 1;
            let mut body = Vec::new();
            while j < lines.len() && lines[j].trim_end_matches('\r') != "```" {
                body.push(lines[j]);
                j += 1;
            }
            blocks.push(body.join("\n"));
            i = j + 1;
        } else {
            i += 1;
        }
    }
    blocks
}

/// Split text into word tokens ([A-Za-z0-9_]+ runs), for keyword-as-a-word
/// scanning (never a substring match, so "state" does not match "stateful").
pub fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|s| !s.is_empty())
}

/// Recursively collect every `.ash` file under `dir` (no skip-list — unlike
/// `ashlar::find_ash_files`, which deliberately excludes `suites/`). Missing
/// directories yield an empty list rather than panicking; callers that need
/// fixtures to exist assert non-emptiness themselves.
pub fn collect_ash_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_ash_files(&path, out);
        } else if path.extension().map(|e| e == "ash").unwrap_or(false) {
            out.push(path);
        }
    }
}

/// `collect_ash_files`, sorted by path.
pub fn ash_files_sorted(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_ash_files(dir, &mut out);
    out.sort();
    out
}

/// One `suites/t_a4` fixture case: either a single `NN-slug.ash` file or a
/// `NN-slug/` directory of `.ash` files, paired with its `NN-slug.error`
/// (the expected diagnostic id).
pub struct T4Case {
    pub name: String,
    pub sources: Vec<(String, String)>,
    pub expected_id: String,
}

/// Gather every case directly under `suites/t_a4`, sorted by entry name.
/// Panics (with a specific message) if the directory itself, or an expected
/// `.error` sidecar, is missing — callers decide whether an empty *set of
/// cases* is acceptable (T-A4 says it is not).
pub fn gather_t_a4_cases(root: &Path) -> Vec<T4Case> {
    let dir = root.join("suites/t_a4");
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", dir.display(), e))
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();

    let mut cases = Vec::new();
    for path in &entries {
        let file_name = path.file_name().unwrap().to_string_lossy().to_string();
        if file_name.ends_with(".error") {
            continue; // paired in below, not a case on its own
        }

        let (base, sources) = if path.is_dir() {
            let base = file_name.clone();
            let files = ash_files_sorted(path);
            let sources: Vec<(String, String)> = files
                .iter()
                .map(|p| {
                    let rel = p
                        .strip_prefix(&dir)
                        .unwrap_or(p)
                        .to_string_lossy()
                        .replace('\\', "/");
                    (rel, read_text(p))
                })
                .collect();
            (base, sources)
        } else if file_name.ends_with(".ash") {
            let base = file_name.trim_end_matches(".ash").to_string();
            (base, vec![(file_name.clone(), read_text(path))])
        } else {
            continue; // stray non-fixture file (e.g. a README) — ignore
        };

        let error_path = dir.join(format!("{}.error", base));
        let expected_id = read_text(&error_path).trim().to_string();

        cases.push(T4Case {
            name: base,
            sources,
            expected_id,
        });
    }
    cases
}

/// Convert a 1-based (line, col) position — columns counting Unicode scalar
/// values, per `tokens::Pos` — to a byte offset into `src`. Used to splice
/// diagnostic edits into source text.
pub fn pos_to_byte_offset(src: &str, pos: &ashlar::tokens::Pos) -> usize {
    let mut line_start = 0usize;
    let mut current_line: u32 = 1;
    if pos.line > 1 {
        let mut found = false;
        for (i, c) in src.char_indices() {
            if c == '\n' {
                current_line += 1;
                if current_line == pos.line {
                    line_start = i + 1;
                    found = true;
                    break;
                }
            }
        }
        if !found {
            return src.len();
        }
    }
    let target = pos.col.saturating_sub(1);
    let mut offset = line_start;
    let mut col_count: u32 = 0;
    for c in src[line_start..].chars() {
        if col_count == target {
            break;
        }
        offset += c.len_utf8();
        col_count += 1;
    }
    offset
}

/// Apply one diagnostic's edits, in memory, to a `path -> contents` map.
/// Edits are grouped by file, then applied within each file from the
/// bottom of the file upward (descending by (line, col)) so that splicing
/// an earlier edit never invalidates the byte offset of a later one.
pub fn apply_edits_to_sources(sources: &mut BTreeMap<String, String>, edits: &[ashlar::diag::Edit]) {
    let mut by_file: BTreeMap<String, Vec<&ashlar::diag::Edit>> = BTreeMap::new();
    for e in edits {
        by_file.entry(e.file.clone()).or_default().push(e);
    }
    for (file, mut file_edits) in by_file {
        file_edits.sort_by(|a, b| (b.start.line, b.start.col).cmp(&(a.start.line, a.start.col)));
        let content = sources
            .get_mut(&file)
            .unwrap_or_else(|| panic!("fix referenced file `{}` not present among fixture sources", file));
        for e in file_edits {
            let start = pos_to_byte_offset(content, &e.start);
            let end = pos_to_byte_offset(content, &e.end);
            assert!(start <= end, "edit for `{}` has start byte {} after end byte {}", file, start, end);
            content.replace_range(start..end, &e.text);
        }
    }
}

/// Replace every occurrence of each path in `paths` with a path-independent
/// placeholder (`F0`, `F1`, ... in sorted order), so two manifests that
/// differ only in recorded file locations compare equal (F3).
pub fn normalize_paths(manifest: &str, paths: &[String]) -> String {
    let mut sorted: Vec<&String> = paths.iter().collect();
    sorted.sort();
    let mut out = manifest.to_string();
    for (i, p) in sorted.iter().enumerate() {
        out = out.replace(p.as_str(), &format!("F{}", i));
    }
    out
}
