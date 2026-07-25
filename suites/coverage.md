# Suite coverage

This is the map from requirement id (`docs/requirements.md`) to the thing that
proves it: a test that runs, or a fixture corpus another test drives. It exists
so "is this requirement covered" never depends on remembering where things
landed.

**Status: every requirement has a running test.** There are no `[planned]` rows.
`cargo test` runs 17 binaries green in debug and release with zero warnings, and
`t_no_stubs` proves there is no `todo!()` anywhere in `src/`. The one requirement
that cannot be a CI job is **A3**: it needs a fresh model with no reference in
context, so it runs by hand via `suites/t_a3/PROTOCOL.md` and its results are
recorded per-run in `suites/t_a3/results/` (currently 25/25, run 4).

A `[runs]` row has real `#[test]`s behind it — in `crates/ashlar/tests/` for the
integration suites, or in a `#[cfg(test)]` module inside the named source file
for the properties that belong to one pipeline stage. A `[fixtures]` row is
corpus data another test drives, named here so the corpus is not invisible.

`t_meta_planned_rows_are_actually_open` enforces the honesty of this file: a
`[planned]` row is only legal if `docs/roadmap.md`'s open section names that
requirement, and a `[runs]` row must point at a file that really contains tests.
That check exists because this file once carried fourteen `[planned]` rows for
requirements that were fully delivered — the path existed, so the old T-META was
satisfied, and the coverage map quietly misreported a third of the requirements.

## By requirement cluster

**A (surface & corpus).** A1 counts the reference's bytes; A5 prints the
per-section budget distribution and fails on a construct over 20%; A2 extracts
every ```ash block from the reference and compiles it, which is also what proves
**C1**. A4 and A6 are the loud-failure corpus — 31 fixtures under `suites/t_a4`,
each a plausible-but-wrong construct paired with the diagnostic it must produce.
A3 is the cold-read gate: fixture data plus a hand-run protocol, with no CI
runner by design (a model that could read the repo would not be cold).

**B (resolution).** B3/B4/B5/B7 are inline fixtures in `t_b.rs` — deliberately
inline so each failure mode (zero-resolution, ambiguous, case-collision,
`use`-of-a-part) is pinned in the test rather than in a fixture that may drift.
B5 also scans every `.ash` in the repo for a path, URL, or port literal, and
covers `foreign.json` keys, the one non-`.ash` file that carries a name the
compiler reasons about. B1 is `t_f.rs`'s relocation test; B2 is the `t_a4`
corpus; B6 is `resolve.rs`'s own unit tests, since the space-header rule is a
parse-time structural property.

**C (composition).** C2/C3 are `resolve.rs` (composition order, the W001
tie-break). C4–C8 are `compose.rs`: the five merge kinds against every value
shape, kind identity across layers, storage identity, and lifecycle-as-`stack`.
`rekind` — the escape hatch from C5's identity rule — is proven in `t_e.rs`.

**D (correction).** D1 and D2 are `t_d.rs`, which applies every fixture's
machine `edits` in memory and rechecks: the proof that a fix resolves what it
targets and introduces nothing new. D5 is `t_d5.rs`, the round-trip metric —
one check → apply → recheck cycle over all 11 machine-fixable fixtures, mean
rounds-to-clean 1.00. D4 is `diag.rs`'s own tests over the JSONL wire format.
D3 is the `t_a4` corpus for its first half (conditions the compiler detects
rather than deferring to runtime); its second half is a documentation
obligation, and the reference states the reason for each condition it leaves to
runtime — division by zero, `!` on `none`, foreign shape and reachability, and
`peruser` with no user.

**E (refactor commands).** All of E1–E6 are `t_e.rs`: blast radius correctness,
absence of the prior state afterwards, refusal when radius cannot be computed,
and reversal. E4 is worth naming precisely — reversal restores the *program*
(same parts and homes, same composition order, a `use` closure that may only
have widened), and byte-identity is asserted separately for the commands that
guarantee it (ADR-0018).

**F (build & determinism).** F2 (delete the manifest and rebuild reproduces it)
and F3 (relocation changes only recorded locations) are `t_f.rs`. F1 is
`t_f1.rs`, a hard-failing latency gate: a single-file change in a 1,000-file
project must re-check under 100ms. It is release-only, because a debug-build
number would not mean anything.

**G (runtime & meta).** G2–G4 are `t_g.rs`'s runtime conformance tests — the
same handler over HTTP and the socket, hot reload preserving state, and the
builtin set including per-user `peruser` scoping and its loud failure with no
user. G1 is `t_meta.rs`, checked two ways: an empty `[dependencies]` table, and
no external crate reachable from the workspace. G5 (no registry) is the same
check plus `vendor`'s copy-in semantics in `t_e.rs` — the absence of version
resolution is proven by there being nothing to resolve.

`t_examples` sits across all of these rather than under one letter: all fifteen
projects compile clean, are canonically formatted, and are driven at runtime
over real HTTP and WebSockets.

## Machine-readable index

The block below is parsed by `t_meta.rs`. Format: `ID -> path [status]`, one
row per requirement id, status one of `runs` / `fixtures` / `planned`. A
`planned` row must be named as open in `docs/roadmap.md`.

<!-- T-META:BEGIN -->
A1 -> crates/ashlar/tests/t_a1.rs [runs]
A2 -> crates/ashlar/tests/t_a2.rs [runs]
A3 -> suites/t_a3 [fixtures]
A4 -> crates/ashlar/tests/t_a4.rs [runs]
A5 -> crates/ashlar/tests/t_a5.rs [runs]
A6 -> crates/ashlar/tests/t_a4.rs [runs]
B1 -> crates/ashlar/tests/t_f.rs [runs]
B2 -> suites/t_a4 [fixtures]
B3 -> crates/ashlar/tests/t_b.rs [runs]
B4 -> crates/ashlar/tests/t_b.rs [runs]
B5 -> crates/ashlar/tests/t_b.rs [runs]
B6 -> crates/ashlar/src/resolve.rs [runs]
B7 -> crates/ashlar/tests/t_b.rs [runs]
C1 -> crates/ashlar/tests/t_a2.rs [runs]
C2 -> crates/ashlar/src/resolve.rs [runs]
C3 -> crates/ashlar/src/resolve.rs [runs]
C4 -> crates/ashlar/src/compose.rs [runs]
C5 -> crates/ashlar/src/compose.rs [runs]
C6 -> crates/ashlar/src/compose.rs [runs]
C7 -> crates/ashlar/src/compose.rs [runs]
C8 -> crates/ashlar/src/compose.rs [runs]
D1 -> crates/ashlar/tests/t_d.rs [runs]
D2 -> crates/ashlar/tests/t_d.rs [runs]
D3 -> crates/ashlar/tests/t_a4.rs [runs]
D4 -> crates/ashlar/src/diag.rs [runs]
D5 -> crates/ashlar/tests/t_d5.rs [runs]
E1 -> crates/ashlar/tests/t_e.rs [runs]
E2 -> crates/ashlar/tests/t_e.rs [runs]
E3 -> crates/ashlar/tests/t_e.rs [runs]
E4 -> crates/ashlar/tests/t_e.rs [runs]
E5 -> crates/ashlar/tests/t_e.rs [runs]
E6 -> crates/ashlar/tests/t_e.rs [runs]
F1 -> crates/ashlar/tests/t_f1.rs [runs]
F2 -> crates/ashlar/tests/t_f.rs [runs]
F3 -> crates/ashlar/tests/t_f.rs [runs]
G1 -> crates/ashlar/tests/t_meta.rs [runs]
G2 -> crates/ashlar/tests/t_g.rs [runs]
G3 -> crates/ashlar/tests/t_g.rs [runs]
G4 -> crates/ashlar/tests/t_g.rs [runs]
G5 -> crates/ashlar/tests/t_meta.rs [runs]
<!-- T-META:END -->
