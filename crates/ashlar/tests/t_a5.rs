//! T-A5 — reference-budget audit (requirement A5): no construct costs
//! reference budget disproportionate to its value.
//!
//! The criterion, defined with this suite (roadmap item 7): the reference
//! is divided at its FINEST heading level — `###` subsections where they
//! exist, `##` sections otherwise — because A5 governs constructs, and a
//! chapter like §9 (the runtime) is eleven constructs, not one. A single
//! finest-grained section consuming more than 20% of the bytes actually
//! used is flagged. A5 is a judgment call in the end — "worth it" cannot
//! be computed — but the audit makes the numbers visible so the judgment
//! is argued from data, and the hard assertion catches runaway growth of
//! any one construct's documentation between revisions.

mod support;

#[test]
fn t_a5_section_budget_distribution() {
    // covers: A5
    let root = support::repo_root();
    let whole = support::read_text(&root.join("AGENTS.md"));

    // A5 governs CONSTRUCTS, so it measures the reference half of AGENTS.md
    // only: the working contract's sections are rules, not language surface,
    // and counting them would both dilute every construct's share and put a
    // workflow paragraph on trial for a language budget (ADR-0030).
    const MARKER: &str = "<!-- REFERENCE:BEGIN -->";
    let start = whole
        .find(MARKER)
        .unwrap_or_else(|| panic!("AGENTS.md is missing the `{}` marker", MARKER));
    let text = &whole[start + MARKER.len()..];
    let total = text.len();

    // Split at `## ` and `### ` headings; a `##` section's own entry
    // covers only its content before the first `###` inside it.
    let mut sections: Vec<(String, usize)> = Vec::new();
    let mut current_name = "preamble".to_string();
    let mut current_len = 0usize;
    for line in text.lines() {
        let heading = line
            .strip_prefix("### ")
            .or_else(|| line.strip_prefix("## "));
        if let Some(h) = heading {
            sections.push((current_name.clone(), current_len));
            current_name = h.trim().to_string();
            current_len = 0;
        }
        current_len += line.len() + 1;
    }
    sections.push((current_name, current_len));

    let mut report = String::new();
    report.push_str(&format!(
        "AGENTS.md reference half: {} bytes; whole file is the 40,000 A1 budget\n",
        total
    ));
    let mut worst: Option<(String, usize)> = None;
    for (name, len) in &sections {
        let pct = *len as f64 * 100.0 / total as f64;
        report.push_str(&format!("  {:5.1}%  {:6}  {}\n", pct, len, name));
        if worst.as_ref().map(|(_, w)| len > w).unwrap_or(true) {
            worst = Some((name.clone(), *len));
        }
    }
    println!("{}", report);

    // Hard ceiling: no single section may consume more than 20% of the
    // bytes actually used. (The runtime section §9 is the natural heavy
    // hitter; if it crosses the line, it splits or slims.)
    let (name, len) = worst.expect("reference has sections");
    let pct = len as f64 * 100.0 / total as f64;
    assert!(
        pct <= 20.0,
        "A5: section `{}` consumes {:.1}% ({} bytes) of the reference — over the 20% ceiling.\n{}",
        name,
        pct,
        len,
        report
    );
}
