//! T-D5 — requirement D5: the number of round trips from "agent writes
//! code" to "code is correct" is the measure of the compiler's quality.
//!
//! This suite answers the roadmap's open question — what counts as a
//! round trip, mechanically? — with: **one check → apply-machine-edits
//! cycle**. An agent that trusts the corrections needs exactly as many
//! round trips as this loop takes. The suite runs the loop over every
//! T-A4 fixture whose diagnostics carry machine edits and asserts:
//!
//! * every machine-fixable fixture converges (no fixable error remains),
//! * within at most 3 rounds (D2's fix-resolves-its-error contract is
//!   asserted per-fix by T-D; independent errors may chain),
//! * and the mean stays near the one-round ideal, because D5 is a metric
//!   to watch, not only a bound to pass.

mod support;

use std::collections::BTreeMap;

#[test]
fn t_d5_round_trips_to_clean_are_bounded_and_reported() {
    // covers: D5
    let root = support::repo_root();
    let mut measured: Vec<(String, usize)> = Vec::new();

    for case in support::gather_t_a4_cases(&root) {
        let mut sources: BTreeMap<String, String> = case.sources.iter().cloned().collect();

        let has_machine_fix = |diags: &[ashlar::diag::Diag]| {
            diags
                .iter()
                .any(|d| d.fix.as_ref().map(|f| !f.edits.is_empty()).unwrap_or(false))
        };

        let r0 = ashlar::check_sources(sources.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
        if !has_machine_fix(&r0.diags) {
            continue; // judgment-required fixtures are D1's territory, not D5's
        }

        let mut rounds = 0usize;
        loop {
            let r = ashlar::check_sources(
                sources.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            );
            let edits: Vec<ashlar::diag::Edit> = r
                .diags
                .iter()
                .filter_map(|d| d.fix.as_ref())
                .flat_map(|f| f.edits.iter().cloned())
                .collect();
            if edits.is_empty() {
                break;
            }
            rounds += 1;
            assert!(rounds <= 3, "{}: did not converge within 3 rounds", case.name);
            support::apply_edits_to_sources(&mut sources, &edits);
        }

        measured.push((case.name.clone(), rounds));
    }

    assert!(
        measured.len() >= 5,
        "expected several machine-fixable fixtures, found {}",
        measured.len()
    );
    let total: usize = measured.iter().map(|(_, r)| r).sum();
    let mean = total as f64 / measured.len() as f64;
    println!(
        "D5: {} machine-fixable fixtures, mean rounds-to-clean {:.2}",
        measured.len(),
        mean
    );
    for (slug, r) in &measured {
        println!("  {} round(s)  {}", r, slug);
    }
    // The one-round ideal holds for the corpus today; a regression that
    // makes fixes cascade shows up here as a mean above 1.
    assert!(
        mean <= 1.5,
        "D5 regression: mean rounds-to-clean {:.2} (expected near 1)",
        mean
    );
}
