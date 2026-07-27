# Suite coverage

This is the map from requirement id (`docs/requirements.md`) to the thing that
proves it: a test that runs, or a fixture corpus another test drives. It exists
so "is this requirement covered" never depends on remembering where things
landed.

**Status: every requirement has an evidence site.** There are no `[planned]` rows.
`cargo test` runs 17 binaries green in debug and release with zero warnings, and
`t_no_stubs` proves there is no `todo!()` anywhere in `src/`. The one requirement
that cannot be a CI job is **A3**: it needs fresh agents in the normal
repository environment, so it runs by hand via `suites/t_a3/PROTOCOL.md` and
records each run in `suites/t_a3/results/`. Runs 1–5 measured the superseded
no-reference question; run 5's 24/25 remains historical evidence, not the result
of the revised baseline-aware gate. Its next run is open in the roadmap.

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
A3 is the agent-read gate: fixture data plus a hand-run protocol, with no CI
runner by design because each fixture needs a fresh agent. `AGENTS.md` carrying
the reference is the baseline under test, not contamination. The rubric and
previous answers are the material withheld from each reader.

**B (resolution).** B3/B4/B5/B7 are inline fixtures in `t_b.rs` — deliberately
inline so each failure mode (zero-resolution, ambiguous, case-collision,
`use`-of-a-part) is pinned in the test rather than in a fixture that may drift.
B5 also scans for a path or URL written into source: every `.ash` in the t_a3
corpus, every ```ash block in the reference, and every `.ash` under `examples/`
(comments stripped there — a comment binds nothing, and two examples name a
sibling file while explaining composition). It covers `foreign.json` keys too,
the one non-`.ash` file carrying a name the compiler reasons about. B5's other
half — a program may *depend* on a location it cannot know, via a `setting`
deployment supplies — is proved in T-G by
`t_g_missing_required_setting_refuses_before_serving` (every gap named with its
shape, refused before a port is bound), in `t_examples`' gallery test (a page of
fifteen addresses whose source has none), and in `settings.rs`'s own unit
tests. B1 is `t_f.rs`'s relocation test; B2 is the `t_a4`
corpus; B6 is `resolve.rs`'s own unit tests, since the space-header rule is a
parse-time structural property.

**C (composition).** C2/C3 are `resolve.rs` (composition order, the W001
tie-break). C9 is `delta.rs`: composition order compared against the previous
manifest, with W002 naming any part whose layers were resequenced by an edit
elsewhere in the use graph — the check that turns C2's determinism into
something an author is actually told about (ADR-0012). C4–C8 are `compose.rs`:
the five merge kinds against every value shape, kind identity across layers,
storage identity, and lifecycle-as-`stack`. `rekind` — the escape hatch from
C5's identity rule — is proven in `t_e.rs`.

**D (correction).** D1 and D2 are `t_d.rs`, which applies every fixture's
machine `edits` in memory and rechecks: the proof that a fix resolves what it
targets and introduces nothing new. D5 is `t_d5.rs`, the round-trip metric —
one check → apply → recheck cycle, reporting both numbers it owes: 10 of the
41 fixtures carry a machine-applicable fix at all (24%), and those converge at
a mean of 1.00 rounds. The fraction is there because the mean alone graded only
the fixtures that already had fixes. D4 is `diag.rs`'s own tests over the JSONL
wire format. D6 is `t_d.rs`: an ambiguous name carries a note naming every
candidate and no edits, and `t_e.rs` drives the case that found it — a rendered
page whose `el(Card, ...)` survives `ashlar fix` unchanged. D3 is the `t_a4`
corpus for its first half (conditions the compiler detects rather than
deferring to runtime); its second half is a documentation
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

`t_examples` sits across all of these rather than under one letter: all
seventeen projects compile clean, are canonically formatted, and are driven at
runtime over real HTTP and WebSockets — including `gallery`, the showcase page,
whose driving test asserts it renders sixteen addresses that appear nowhere in
its source.

Two of those projects are driven against a co-process rather than the system
they name — `abacus` against Python, `enclave` against a stand-in for the mesh
daemons — and both skip with a printed reason when the co-process's language is
absent. `enclave` also carries the G5 half `vendor` cannot check: its
`vendor/mesh/` is asserted byte-identical to `lib/mesh/`, because a vendored
dependency that drifts from its source is the version skew a registry exists to
manage and this language refuses to have. What the stand-in cannot prove — a
second machine — is named in `docs/roadmap.md` rather than implied by a green
run.

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
C9 -> crates/ashlar/src/delta.rs [runs]
D1 -> crates/ashlar/tests/t_d.rs [runs]
D2 -> crates/ashlar/tests/t_d.rs [runs]
D3 -> crates/ashlar/tests/t_a4.rs [runs]
D4 -> crates/ashlar/src/diag.rs [runs]
D5 -> crates/ashlar/tests/t_d5.rs [runs]
D6 -> crates/ashlar/tests/t_d.rs [runs]
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
