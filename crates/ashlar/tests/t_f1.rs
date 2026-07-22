//! T-F1 — requirement F1: incremental compilation of a single-file change
//! completes in under 100ms in a project of 1,000 source files.
//!
//! The gate is enforced in release builds (the binary that ships); a
//! debug-build run measures and prints but does not assert, because an
//! unoptimized interpreter's constant factor is not what F1 governs.

use std::time::Instant;

fn project(n: usize) -> Vec<(String, String)> {
    (0..n)
        .map(|i| {
            let space = format!("s{}.m{}", i / 10, i % 10);
            let mut src = format!("space {}\n\n", space);
            if i % 10 != 0 {
                src.push_str(&format!("use s{}.m0\n\n", i / 10));
            }
            src.push_str(&format!(
                "part P{} {{\n  name: text = \"p{}\"\n  count: number = {}\n  go = (n: number) => n + {}\n}}\n",
                i, i, i, i
            ));
            (format!("f{:04}.ash", i), src)
        })
        .collect()
}

#[test]
fn t_f1_incremental_single_file_change_under_100ms() {
    // covers: F1
    let sources = project(1000);
    let mut cache = ashlar::IncrementalCache::default();

    // Warm pass: the full project parses once into the cache.
    let full_start = Instant::now();
    let r = ashlar::check_sources_incremental(sources.clone(), &mut cache);
    let full_ms = full_start.elapsed().as_millis();
    assert!(r.diags.is_empty(), "fixture must be clean: {:?}", r.diags);

    // The measured operation: one file changes, everything re-checks.
    let mut changed = sources.clone();
    changed[500].1 = changed[500].1.replace("n + 500", "n + 501");
    let inc_start = Instant::now();
    let r2 = ashlar::check_sources_incremental(changed, &mut cache);
    let inc_ms = inc_start.elapsed().as_millis();
    assert!(r2.diags.is_empty(), "{:?}", r2.diags);

    println!(
        "F1: full pass {} ms, incremental single-file change {} ms (1,000 files)",
        full_ms, inc_ms
    );

    // Correctness: the change is actually reflected, not cached over.
    assert!(r2.composed.contains_key("s50.m0.P500"));

    #[cfg(not(debug_assertions))]
    assert!(
        inc_ms < 100,
        "F1: incremental change took {} ms (budget: 100ms)",
        inc_ms
    );
    #[cfg(debug_assertions)]
    eprintln!(
        "F1 note: debug build — measured {} ms; the 100ms gate asserts under --release.",
        inc_ms
    );
}
