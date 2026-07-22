//! T-A4 — loud failure: every case under suites/t_a4 must produce the
//! specific ERROR-level diagnostic its `.error` sidecar names.

mod support;

#[test]
fn t_a4_every_case_produces_its_expected_error() {
    // covers: A4, A6
    let root = support::repo_root();
    let cases = support::gather_t_a4_cases(&root);

    assert!(
        !cases.is_empty(),
        "suites/t_a4 has no cases — T-A4 (loud failure) requires at least one fixture; an empty \
         t_a4 directory is itself a failure, not a pass"
    );

    let mut failures = Vec::new();
    for case in &cases {
        let result = ashlar::check_sources(case.sources.clone());
        let matched = result
            .diags
            .iter()
            .any(|d| d.is_error() && d.id == case.expected_id);
        if !matched {
            let rendered: Vec<String> = result.diags.iter().map(|d| d.human()).collect();
            failures.push(format!(
                "case `{}`: expected an ERROR-level {} diagnostic, got {} diagnostic(s):\n{}",
                case.name,
                case.expected_id,
                result.diags.len(),
                if rendered.is_empty() {
                    "  (none)".to_string()
                } else {
                    rendered.iter().map(|r| format!("  {}", r)).collect::<Vec<_>>().join("\n")
                }
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} t_a4 case(s) did not fail loudly as expected:\n\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n\n")
    );
}
