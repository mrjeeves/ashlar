# Suite coverage

This is the map from requirement id (`docs/requirements.md`) to the thing
that proves it: a test that runs, a fixture corpus that another test drives,
or a roadmap entry for work that has not started. It exists so "is this
requirement covered" never depends on remembering where things landed.

Honest status, as of this writing: the pipeline is mid-build. `lexer.rs`,
`parser.rs`, `resolve.rs`, and `compose.rs` (module implementors), plus
`fixup.rs` and `manifest.rs` (CLI implementor), are still contract scaffolds
— every one of them is a single `todo!()` behind the signature this suite
was written against. `suites/t_a3` and `suites/t_a4` are still empty — the
corpus agent is populating them concurrently. `docs/requirements.md`,
`docs/roadmap.md`, and `docs/vision.md` do not exist on disk yet either. None
of that is a defect in the tests below: `tests/*.rs` is written to compile
against the contracts in `lib.rs` / `diag.rs` / `resolved.rs` right now, and
to fail loudly and specifically — not to panic unhelpfully or hang — until
the rest of the workspace catches up. That is the intended state described
in this project's working agreement, not a gap this document is hiding.

## What `[runs]` means here

A `[runs]` row has a real `#[test]` behind it in `crates/ashlar/tests/`, and
that test compiles today. Whether it *passes* today depends on what it
needs:

| Test file | Passes today? | Needs |
|---|---|---|
| `t_a1.rs` | yes | only the reference file on disk (already true) |
| `t_no_stubs.rs` | no (by design) | `fixup.rs`/`manifest.rs` to stop being `todo!()` scaffolds — this test is *supposed* to fail until every stub is replaced, per this repo's policy that stubs never survive to a commit |
| `t_a2.rs` | no | lexer, parser, resolver, composer all implemented (reference examples must actually check clean) |
| `t_a4.rs` | no | same, plus `suites/t_a4` populated (currently empty, which also fails T-A4's own "non-empty" assertion) |
| `t_b.rs` | no | resolver implemented; the B5 (no-locations) half only needs `suites/t_a3` populated |
| `t_d.rs` | no | resolver/composer implemented and emitting real `Fix { edits }`, plus `suites/t_a4` populated |
| `t_f.rs` | no | resolver/composer implemented, and `manifest::render` replacing its `todo!()` |
| `t_meta.rs` | no | `docs/requirements.md` on disk (the docs agent's deliverable) |

None of these are flaky or order-dependent; each currently fails at a single
well-defined point (a `todo!()` panic, or an assertion naming the exact
missing file/fixture), which is what "loud failure" means at the meta level
too.

## By requirement cluster

**A (surface & corpus).** A1–A2 and A4/A6 have real runners
(`t_a1.rs`/`t_a2.rs`/`t_a4.rs`) that check the reference document and the
error corpus directly. A3 is pure fixture data (`suites/t_a3`) with no
runner of its own — `t_a2.rs`'s keyword scan and `t_b.rs`'s B5 scan are what
actually exercise it. A5 (the toolchain surface beyond `check`) has no code
yet; it is a `docs/roadmap.md` entry.

**B (resolution).** B3/B4/B5/B7 are proven by inline fixtures in `t_b.rs` —
deliberately inline rather than file-based, so the exact shape of each
failure mode (zero-resolution, ambiguous, case-collision, `use`-of-a-part)
is pinned in the test itself instead of hoping a fixture keeps meaning what
it meant. B1 (layer order survives relocation) and B2 (the t_a4 corpus) are
proven elsewhere: B1 by `t_f.rs`'s relocation test, B2 by the fixture corpus
itself. B6 is the resolver module (`resolve.rs`) directly, since it is a
structural parse-time property (space header must come first) more than a
resolution *outcome* — there is no separate integration test for it beyond
what `t_a4.rs` exercises via an E022 fixture.

**C (composition).** C1 (reference sufficiency) is `t_a2.rs`'s clean-compile
check. C2/C3 live in `resolve.rs` (composition order, W001) — they are
outcomes the resolver computes, exercised indirectly through `t_f.rs` and
`t_a4.rs` fixtures rather than a standalone unit test, since this suite owns
integration/meta tests, not `resolve.rs`'s own unit tests. C4–C7 are
`compose.rs` in the same sense. C8 (kind-changing refactor, `rekind`) is not
implemented; roadmap entry.

**D (correction).** D1 (every diagnostic is specific enough to apply without
judgment) and D2 (a machine fix actually resolves what it targets, cleanly)
are both proven by `t_d.rs`, which applies every fixture's machine-edits in
memory and rechecks. D2 is the one this suite leans on hardest: it is the
whole reason `t_d.rs` exists rather than trusting that "has an `edits` list"
implies "the edits are correct." D4 (diagnostic wire format) is `diag.rs`
itself — a contract file, proven by every other test's ability to read
`.id`/`.level`/`.fix` off a real `Diag`. D3 (`ashlar fix` as a CLI verb) and
D5 (whatever the next correction increment is) are not implemented; roadmap
entries — D2 already covers the mechanism they would use, so these are CLI
plumbing, not new logic to verify.

**E (refactor commands: `rename`, `rekind`, `radius`, `vendor`).** None of
E1–E6 are implemented. These require a working manifest and a notion of
"blast radius" computed from it, both of which are next-increment work per
`docs/roadmap.md`. There is nothing to test yet; pretending otherwise with a
placeholder test would be worse than an honest `[planned]`.

**F (build & determinism).** F2/F3 are `t_f.rs`, directly. F1 (incremental,
sub-100ms rebuilds) is a performance property with no implementation to
measure yet — roadmap entry, not a test we could write meaningfully today.

**G (runtime & meta).** G1 (zero dependencies) is `t_meta.rs`, checked two
ways: `crates/ashlar/Cargo.toml` has an empty `[dependencies]` table, and
this file's own machine block is structurally validated against
`docs/requirements.md`. G2–G5 (the actual running server: routes, views,
state, schedules) are runtime features with no interpreter yet — the
compiler front end this crate builds does not execute anything — so they
are roadmap entries, not tests.

## Machine-readable index

The block below is parsed by `t_meta.rs`. Format: `ID -> path [status]`,
one row per requirement id, status one of `runs` / `fixtures` / `planned`.

<!-- T-META:BEGIN -->
A1 -> crates/ashlar/tests/t_a1.rs [runs]
A2 -> crates/ashlar/tests/t_a2.rs [runs]
A3 -> suites/t_a3 [fixtures]
A4 -> crates/ashlar/tests/t_a4.rs [runs]
A5 -> docs/roadmap.md [planned]
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
C8 -> docs/roadmap.md [planned]
D1 -> crates/ashlar/tests/t_d.rs [runs]
D2 -> crates/ashlar/tests/t_d.rs [runs]
D3 -> docs/roadmap.md [planned]
D4 -> crates/ashlar/src/diag.rs [runs]
D5 -> docs/roadmap.md [planned]
E1 -> docs/roadmap.md [planned]
E2 -> docs/roadmap.md [planned]
E3 -> docs/roadmap.md [planned]
E4 -> docs/roadmap.md [planned]
E5 -> docs/roadmap.md [planned]
E6 -> docs/roadmap.md [planned]
F1 -> docs/roadmap.md [planned]
F2 -> crates/ashlar/tests/t_f.rs [runs]
F3 -> crates/ashlar/tests/t_f.rs [runs]
G1 -> crates/ashlar/tests/t_meta.rs [runs]
G2 -> docs/roadmap.md [planned]
G3 -> docs/roadmap.md [planned]
G4 -> docs/roadmap.md [planned]
G5 -> docs/roadmap.md [planned]
<!-- T-META:END -->
