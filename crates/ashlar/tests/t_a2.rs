//! T-A2 — reference sufficiency: every example in the reference compiles
//! clean, and no fixture exercises a construct the reference does not teach.

mod support;

#[test]
fn t_a2_reference_examples_compile_with_zero_diagnostics() {
    // covers: A2, C1
    let root = support::repo_root();
    let reference_path = root.join("reference/ashlar.md");
    let text = support::read_text(&reference_path);

    let blocks = support::extract_ash_blocks(&text);
    assert!(!blocks.is_empty(), "found no ```ash fenced blocks in reference/ashlar.md");

    let sources: Vec<(String, String)> = blocks
        .into_iter()
        .enumerate()
        .map(|(i, body)| (format!("block{:02}.ash", i + 1), body))
        .collect();

    let result = ashlar::check_sources(sources);

    if !result.diags.is_empty() {
        let rendered: Vec<String> = result.diags.iter().map(|d| d.human()).collect();
        panic!(
            "reference examples produced {} diagnostic(s) when checked together as one project \
             (A2: the reference must be sufficient on its own):\n{}",
            result.diags.len(),
            rendered.join("\n")
        );
    }
}

#[test]
fn t_a2_fixture_keywords_are_all_taught_by_the_reference() {
    // covers: A2
    let root = support::repo_root();
    let reference_text = support::read_text(&root.join("reference/ashlar.md"));

    const KEYWORDS: &[&str] = &[
        "space", "use", "part", "foreign", "state", "stored", "synced", "append", "deep", "stack",
        "pipe", "reverse", "let", "if", "else", "for", "in", "return",
    ];

    let mut fixture_files = support::ash_files_sorted(&root.join("suites/t_a3"));
    fixture_files.extend(support::ash_files_sorted(&root.join("suites/t_a4")));
    fixture_files.sort();

    let reference_words: std::collections::BTreeSet<&str> = support::words(&reference_text).collect();

    let mut checked_files = 0usize;
    for f in &fixture_files {
        let content = support::read_text(f);
        checked_files += 1;
        let fixture_words: std::collections::BTreeSet<&str> = support::words(&content).collect();
        for kw in KEYWORDS {
            if fixture_words.contains(kw) && !reference_words.contains(kw) {
                panic!(
                    "{}: keyword `{}` appears in a fixture but not in reference/ashlar.md \
                     (A2: no construct outside the reference)",
                    f.display(),
                    kw
                );
            }
        }
    }

    // Not asserting checked_files > 0: the corpus agent is still writing
    // suites/t_a3 and suites/t_a4, so an empty corpus here is expected
    // before integration, and this check is then vacuously (and correctly)
    // satisfied. T-A4 elsewhere is the test that demands t_a4 be non-empty.
    let _ = checked_files;
}
