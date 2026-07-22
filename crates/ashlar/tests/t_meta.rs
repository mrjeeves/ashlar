//! T-META — coverage: every requirement id docs/requirements.md defines has
//! a row in suites/coverage.md pointing at a real path, and the workspace
//! keeps its zero-dependency promise (G1).

mod support;

use std::collections::BTreeSet;

/// Every occurrence of `**X.**` where X matches `[A-G][0-9]+`, e.g. `**A1.**`.
fn extract_requirement_ids(text: &str) -> BTreeSet<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut ids = BTreeSet::new();
    let mut i = 0;
    while i + 1 < chars.len() {
        if chars[i] == '*' && chars[i + 1] == '*' {
            let mut j = i + 2;
            if j < chars.len() && ('A'..='G').contains(&chars[j]) {
                let letter = chars[j];
                j += 1;
                let digit_start = j;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
                if j > digit_start && j + 2 < chars.len() && chars[j] == '.' && chars[j + 1] == '*' && chars[j + 2] == '*'
                {
                    let id: String = std::iter::once(letter).chain(chars[digit_start..j].iter().copied()).collect();
                    ids.insert(id);
                    i = j + 3;
                    continue;
                }
            }
        }
        i += 1;
    }
    ids
}

struct CoverageRow {
    id: String,
    path: String,
    status: String,
}

/// Parse the lines between the T-META markers in suites/coverage.md, of the
/// form `ID -> path [status]`.
fn parse_coverage_rows(text: &str) -> Vec<CoverageRow> {
    const BEGIN: &str = "<!-- T-META:BEGIN -->";
    const END: &str = "<!-- T-META:END -->";
    let start = text
        .find(BEGIN)
        .unwrap_or_else(|| panic!("suites/coverage.md is missing the `{}` marker", BEGIN));
    let end = text
        .find(END)
        .unwrap_or_else(|| panic!("suites/coverage.md is missing the `{}` marker", END));
    assert!(start < end, "T-META markers are out of order in suites/coverage.md");

    let block = &text[start + BEGIN.len()..end];
    let mut rows = Vec::new();
    for line in block.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let arrow = line
            .find("->")
            .unwrap_or_else(|| panic!("malformed T-META row (no `->`): `{}`", line));
        let id = line[..arrow].trim().to_string();
        let rest = line[arrow + 2..].trim();
        let lb = rest
            .rfind('[')
            .unwrap_or_else(|| panic!("malformed T-META row (no `[status]`): `{}`", line));
        let rb = rest
            .rfind(']')
            .unwrap_or_else(|| panic!("malformed T-META row (no closing `]`): `{}`", line));
        assert!(lb < rb, "malformed T-META row (bracket order): `{}`", line);
        let path = rest[..lb].trim().to_string();
        let status = rest[lb + 1..rb].trim().to_string();
        rows.push(CoverageRow { id, path, status });
    }
    rows
}

#[test]
fn t_meta_requirement_ids_exist_and_are_plentiful() {
    // covers: G1 (documentation half: requirement ids must actually exist)
    let root = support::repo_root();
    let requirements_path = root.join("docs/requirements.md");
    let text = std::fs::read_to_string(&requirements_path).unwrap_or_else(|e| {
        panic!(
            "docs/requirements.md is missing or unreadable ({}) — the docs agent has not landed it \
             yet; T-META cannot verify coverage without it",
            e
        )
    });

    let ids = extract_requirement_ids(&text);
    assert!(
        ids.len() >= 35,
        "expected at least 35 requirement ids of the form **X.** in docs/requirements.md, found {}: {:?}",
        ids.len(),
        ids
    );
}

#[test]
fn t_meta_coverage_table_is_complete_and_accurate() {
    // covers: G1
    let root = support::repo_root();

    let requirements_path = root.join("docs/requirements.md");
    let requirements_text = std::fs::read_to_string(&requirements_path).unwrap_or_else(|e| {
        panic!(
            "docs/requirements.md is missing or unreadable ({}) — T-META cannot cross-check \
             suites/coverage.md without it",
            e
        )
    });
    let requirement_ids = extract_requirement_ids(&requirements_text);
    assert!(!requirement_ids.is_empty(), "docs/requirements.md yielded zero requirement ids");

    let coverage_path = root.join("suites/coverage.md");
    let coverage_text = std::fs::read_to_string(&coverage_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", coverage_path.display(), e));
    let rows = parse_coverage_rows(&coverage_text);
    assert!(!rows.is_empty(), "suites/coverage.md's T-META block has no rows");

    const VALID_STATUSES: &[&str] = &["runs", "fixtures", "planned"];
    let mut row_ids: BTreeSet<String> = BTreeSet::new();
    let mut missing_paths = Vec::new();
    let mut bad_statuses = Vec::new();
    let mut ids_not_in_requirements = Vec::new();

    for row in &rows {
        row_ids.insert(row.id.clone());
        if !VALID_STATUSES.contains(&row.status.as_str()) {
            bad_statuses.push(format!("{} -> {} [{}]", row.id, row.path, row.status));
        }
        if !root.join(&row.path).exists() {
            missing_paths.push(format!("{} -> {} (no such path on disk)", row.id, row.path));
        }
        if !requirement_ids.contains(&row.id) {
            ids_not_in_requirements.push(row.id.clone());
        }
    }

    assert!(
        bad_statuses.is_empty(),
        "coverage rows with a status outside {{runs, fixtures, planned}}:\n{}",
        bad_statuses.join("\n")
    );
    assert!(
        ids_not_in_requirements.is_empty(),
        "coverage rows reference ids docs/requirements.md does not define: {:?}",
        ids_not_in_requirements
    );

    let mut missing_ids: Vec<&String> = requirement_ids.difference(&row_ids).collect();
    missing_ids.sort();
    assert!(
        missing_ids.is_empty(),
        "requirement id(s) with no row in suites/coverage.md: {:?}",
        missing_ids
    );

    assert!(
        missing_paths.is_empty(),
        "coverage rows pointing at paths that do not exist on disk:\n{}",
        missing_paths.join("\n")
    );
}

#[test]
fn t_meta_g1_zero_dependencies() {
    // covers: G1
    let root = support::repo_root();
    let cargo_toml_path = root.join("crates/ashlar/Cargo.toml");
    let cargo_toml = support::read_text(&cargo_toml_path);

    let marker = "[dependencies]";
    let dep_idx = cargo_toml
        .find(marker)
        .unwrap_or_else(|| panic!("{} has no [dependencies] section", cargo_toml_path.display()));
    let after = &cargo_toml[dep_idx + marker.len()..];
    let section_end = after.find("\n[").unwrap_or(after.len());
    let section = &after[..section_end];

    for line in section.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        panic!(
            "G1 violation: crates/ashlar/Cargo.toml declares a dependency ({}), but the crate is \
             required to have zero external dependencies",
            line
        );
    }
}

#[test]
fn t_meta_core_docs_exist() {
    // covers: G1
    let root = support::repo_root();
    for rel in ["docs/requirements.md", "docs/vision.md", "reference/ashlar.md"] {
        let path = root.join(rel);
        assert!(path.exists(), "expected {} to exist", path.display());
    }
}
