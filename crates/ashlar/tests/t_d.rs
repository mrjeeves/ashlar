//! T-D — correction. D2 is the highest-leverage requirement here: every
//! machine-applicable fix, applied in memory, must actually resolve the
//! diagnostic it targets without introducing a new error.

mod support;

use std::collections::{BTreeMap, HashSet};

#[test]
fn t_d2_machine_fixes_resolve_without_introducing_new_errors() {
    // covers: D1, D2
    let root = support::repo_root();
    let cases = support::gather_t_a4_cases(&root);
    assert!(
        !cases.is_empty(),
        "suites/t_a4 has no cases — T-D needs the same fixtures T-A4 uses to exercise fixes"
    );

    let mut fixtures_with_fix = 0usize;

    for case in &cases {
        let original_sources: BTreeMap<String, String> = case.sources.iter().cloned().collect();
        let first = ashlar::check_sources(case.sources.clone());
        let original_error_ids: HashSet<&str> =
            first.diags.iter().filter(|d| d.is_error()).map(|d| d.id).collect();

        let mut exercised_this_case = false;

        for diag in &first.diags {
            let edits = match &diag.fix {
                Some(fix) if !fix.edits.is_empty() => &fix.edits,
                _ => continue,
            };
            exercised_this_case = true;

            let mut fixed_sources = original_sources.clone();
            support::apply_edits_to_sources(&mut fixed_sources, edits);
            let fixed_vec: Vec<(String, String)> = fixed_sources.into_iter().collect();
            let second = ashlar::check_sources(fixed_vec);

            // (1) the exact diagnostic instance we fixed must not remain at
            // its original location.
            let still_present = second
                .diags
                .iter()
                .any(|d2| d2.id == diag.id && d2.file == diag.file && d2.span.start == diag.span.start);
            assert!(
                !still_present,
                "case `{}`: applying the fix for {} at {}:{}:{} did not resolve it — it (or its \
                 twin) is still reported at the same location after the fix",
                case.name, diag.id, diag.file, diag.span.start.line, diag.span.start.col
            );

            // (2) no new error id (absent from the first run) may appear.
            for d2 in second.diags.iter().filter(|d| d.is_error()) {
                assert!(
                    original_error_ids.contains(d2.id),
                    "case `{}`: applying the fix for {} introduced a new error id {} \
                     (D2: a machine fix must not introduce a new error): {:?}",
                    case.name,
                    diag.id,
                    d2.id,
                    d2
                );
            }
        }

        if exercised_this_case {
            fixtures_with_fix += 1;
        }
    }

    println!(
        "T-D2: {} of {} t_a4 fixture(s) exercised at least one machine-applicable fix",
        fixtures_with_fix,
        cases.len()
    );
    assert!(
        fixtures_with_fix >= 5,
        "expected at least 5 t_a4 fixtures to carry a machine-applicable fix (the catalog \
         guarantees edits for at least E010/E011/E004/E005/E020), got {}",
        fixtures_with_fix
    );
}
