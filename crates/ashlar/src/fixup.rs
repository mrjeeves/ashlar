//! Fix applier (reference §8/§11, requirement D2).
//!
//! `ashlar fix` collects every `Fix.edits` entry attached to a check's
//! diagnostics and rewrites the named files in place. Positions are
//! `tokens::Pos`: 1-based, and columns count Unicode scalar values, not
//! bytes — so every edit is converted through a per-line character index
//! before it becomes a byte range a `String` can be sliced or spliced at.
//!
//! Edits within one file are applied in descending (line, col) order so
//! that applying an earlier edit never shifts the position an already-
//! computed later edit was aimed at: every byte range is computed once,
//! against the file's original contents, then spliced back-to-front.
//! Edits whose byte ranges overlap are mutually exclusive — only the first
//! one encountered in that descending order is kept; the rest are skipped
//! with a warning on stderr. Each file is read once and, on success,
//! written once with its complete new contents, so a file is never left
//! partially rewritten.

use crate::diag::{Diag, Edit};
use crate::tokens::Pos;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

/// Apply every machine-applicable fix in `diags` to the files under `root`.
/// Returns the paths (relative, as recorded on the edits) of every file
/// actually rewritten, sorted.
pub fn apply_fixes(root: &Path, diags: &[Diag]) -> io::Result<Vec<String>> {
    let mut by_file: BTreeMap<String, Vec<&Edit>> = BTreeMap::new();
    for d in diags {
        if let Some(fix) = &d.fix {
            for e in &fix.edits {
                by_file.entry(e.file.clone()).or_default().push(e);
            }
        }
    }

    let mut rewritten = Vec::new();
    for (file, mut edits) in by_file {
        // Descending by source position: the edit nearest the end of the
        // file is applied first.
        edits.sort_by(|a, b| (b.start.line, b.start.col).cmp(&(a.start.line, a.start.col)));

        let path = root.join(&file);
        let content = fs::read_to_string(&path)?;
        let line_starts = line_start_offsets(&content);

        // Compute every edit's byte range against the untouched original
        // content, keeping only those that don't overlap an already-kept
        // (and therefore later-in-file) range.
        let mut kept: Vec<(usize, usize, &str)> = Vec::new();
        for e in &edits {
            let start = pos_to_byte(&content, &line_starts, e.start);
            let end = pos_to_byte(&content, &line_starts, e.end);
            let (start, end) = if start <= end { (start, end) } else { (end, start) };

            let overlaps = kept.iter().any(|&(s, en, _)| start < en && s < end);
            if overlaps {
                eprintln!(
                    "warning: skipping overlapping edit in {} at {}:{}",
                    file, e.start.line, e.start.col
                );
                continue;
            }
            kept.push((start, end, e.text.as_str()));
        }

        // `kept` is already in descending byte order: it was built from
        // `edits`, sorted descending by (line, col), and (line, col) order
        // agrees with byte order for positions within one file.
        let mut new_content = content;
        for (start, end, text) in &kept {
            new_content.replace_range(*start..*end, text);
        }

        fs::write(&path, new_content.as_bytes())?;
        rewritten.push(file);
    }

    rewritten.sort();
    Ok(rewritten)
}

/// Byte offset of the start of each 1-based source line. Index `i` (0-based)
/// holds line `i + 1`'s start; `starts[0]` is always `0`.
fn line_start_offsets(content: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, b) in content.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

/// Convert a 1-based `(line, col)` position — columns counting Unicode
/// scalar values — to a byte offset into `content`. A column past the end
/// of its line (as an edit's `end` legitimately is, one past the last
/// character) clamps to the line's end, before its terminating newline.
fn pos_to_byte(content: &str, line_starts: &[usize], pos: Pos) -> usize {
    let line_idx = (pos.line.max(1) as usize) - 1;
    let line_start = *line_starts.get(line_idx).unwrap_or(&content.len());
    let line_end = line_starts
        .get(line_idx + 1)
        .map(|&s| s - 1) // back off the '\n' that ended the previous line
        .unwrap_or(content.len())
        .max(line_start);
    let line = &content[line_start..line_end];

    let col_idx = (pos.col.max(1) as usize) - 1;
    for (ci, (bi, _)) in line.char_indices().enumerate() {
        if ci == col_idx {
            return line_start + bi;
        }
    }
    // Column at or past the line's character count: the position right
    // after the last character (also correct for an empty line).
    line_start + line.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::{Level, E010_SEMICOLON};
    use crate::tokens::Span;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn edit(file: &str, sl: u32, sc: u32, el: u32, ec: u32, text: &str) -> Edit {
        Edit {
            file: file.to_string(),
            start: Pos { line: sl, col: sc },
            end: Pos { line: el, col: ec },
            text: text.to_string(),
        }
    }

    fn diag_with_edits(edits: Vec<Edit>) -> Diag {
        Diag::new(
            E010_SEMICOLON,
            Level::Error,
            "unused",
            Span::point(1, 1),
            "test diagnostic".to_string(),
        )
        .with_fix("test fix".to_string(), edits)
    }

    /// A fresh, empty directory under the OS temp dir, unique per call so
    /// parallel `cargo test` runs never collide.
    fn temp_root(tag: &str) -> std::path::PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ashlar_fixup_{}_{}_{}", tag, nanos, n));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn multi_edit_single_file_applies_descending() {
        // Two edits on one line, computed against the *original* text.
        // Applying them in source order (ascending) would let the first
        // edit's length change shift the second edit's columns out from
        // under it; descending application must avoid that.
        let root = temp_root("descending");
        fs::write(root.join("f.ash"), "abc def\n").unwrap();

        let d = diag_with_edits(vec![
            edit("f.ash", 1, 1, 1, 4, "ABCX"), // "abc" -> "ABCX" (grows)
            edit("f.ash", 1, 5, 1, 8, "D"),    // "def" -> "D"
        ]);

        let rewritten = apply_fixes(&root, &[d]).unwrap();
        assert_eq!(rewritten, vec!["f.ash".to_string()]);
        assert_eq!(fs::read_to_string(root.join("f.ash")).unwrap(), "ABCX D\n");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn insertion_start_equals_end() {
        let root = temp_root("insert");
        fs::write(root.join("f.ash"), "space a\npart X {\n}\n").unwrap();

        // Insert a `use` line right after the space header (W001-style fix).
        let d = diag_with_edits(vec![edit("f.ash", 2, 1, 2, 1, "use b\n")]);

        apply_fixes(&root, &[d]).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("f.ash")).unwrap(),
            "space a\nuse b\npart X {\n}\n"
        );

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn deletion_empty_text() {
        let root = temp_root("delete");
        fs::write(root.join("f.ash"), "let x = 1;\n").unwrap();

        // Columns: l1 e2 t3 (space)4 x5 (space)6 =7 (space)8 1 9 ;10 \n11 —
        // delete the semicolon at column 10 (E010's fix).
        let d = diag_with_edits(vec![edit("f.ash", 1, 10, 1, 11, "")]);

        apply_fixes(&root, &[d]).unwrap();
        assert_eq!(fs::read_to_string(root.join("f.ash")).unwrap(), "let x = 1\n");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn overlapping_edit_is_skipped() {
        let root = temp_root("overlap");
        fs::write(root.join("f.ash"), "abcdef\n").unwrap();

        // A: cols 1-3 ("abc"). B: cols 2-4 ("bcd"). They overlap on cols 2-3.
        // Descending order visits B (col 2) before A (col 1); B is kept, A
        // is skipped as overlapping.
        let d = diag_with_edits(vec![
            edit("f.ash", 1, 1, 1, 4, "X"),
            edit("f.ash", 1, 2, 1, 5, "Y"),
        ]);

        let rewritten = apply_fixes(&root, &[d]).unwrap();
        assert_eq!(rewritten, vec!["f.ash".to_string()]);
        assert_eq!(fs::read_to_string(root.join("f.ash")).unwrap(), "aYef\n");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn multi_file_rewrites_all_and_returns_sorted_paths() {
        let root = temp_root("multifile");
        fs::write(root.join("b.ash"), "one;\n").unwrap();
        fs::write(root.join("a.ash"), "two;\n").unwrap();

        let d = diag_with_edits(vec![
            edit("b.ash", 1, 4, 1, 5, ""),
            edit("a.ash", 1, 4, 1, 5, ""),
        ]);

        let rewritten = apply_fixes(&root, &[d]).unwrap();
        assert_eq!(rewritten, vec!["a.ash".to_string(), "b.ash".to_string()]);
        assert_eq!(fs::read_to_string(root.join("a.ash")).unwrap(), "two\n");
        assert_eq!(fs::read_to_string(root.join("b.ash")).unwrap(), "one\n");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn unicode_columns_count_chars_after_accented_letter() {
        // 'é' is one column but two UTF-8 bytes. A byte-indexed (wrong)
        // implementation would land one byte short and either corrupt the
        // replacement or panic on a non-UTF-8-boundary slice.
        let root = temp_root("unicode-accent");
        fs::write(root.join("f.ash"), "café bar\n").unwrap();

        // Columns: c1 a2 f3 é4 (space)5 b6 a7 r8 \n9 — replace "bar" (6..9).
        let d = diag_with_edits(vec![edit("f.ash", 1, 6, 1, 9, "BAZ")]);

        apply_fixes(&root, &[d]).unwrap();
        assert_eq!(fs::read_to_string(root.join("f.ash")).unwrap(), "café BAZ\n");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn unicode_columns_count_chars_after_cjk() {
        // Each CJK character here is one column but three UTF-8 bytes.
        let root = temp_root("unicode-cjk");
        fs::write(root.join("f.ash"), "日本語 end\n").unwrap();

        // Columns: 1 2 3 (space)4 e5 n6 d7 \n8 — replace "end" (5..8).
        let d = diag_with_edits(vec![edit("f.ash", 1, 5, 1, 8, "END")]);

        apply_fixes(&root, &[d]).unwrap();
        assert_eq!(fs::read_to_string(root.join("f.ash")).unwrap(), "日本語 END\n");

        fs::remove_dir_all(&root).ok();
    }
}
