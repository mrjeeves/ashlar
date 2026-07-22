# Roadmap

An honest "not yet" ledger. Each item names the requirements it will
satisfy and the test that will prove it. A "planned" row anywhere in the
suite tree — including `suites/coverage.md` if and when one exists — is a
debt-ledger entry, not coverage. Nothing on this page is done; when an item
is done, it moves off this page and its test starts running for real,
because a requirement with no passing test is not a satisfied requirement
(T-META).

## 1. Shape checker — DONE 2026-07-22 (moves off this page next revision)

Delivered as `crates/ashlar/src/check.rs`: E006 with expected/actual
shapes stated, mechanical fixes where safe (`text(...)` wrap on mixed
`+`, `!= none` on optional conditions), truthiness enforcement, data-
shape literal checking (field presence/extraness/per-field shapes),
optional-index misuse as a correction (the ADR-0008 F3 conversion), std
and foreign call signatures, and `every` duration validation. Proof:
13 checker unit tests + T-A4 fixtures 31–34 + every reference example
checking clean under it (T-A2). Not yet covered (stays on the D3
inventory): stack/pipe cross-layer shape agreement, route path shape
rules (E021 territory), and deeper inference for unannotated recursion —
all currently `Unknown`-permissive by design.

## 2. Evaluator and runtime

Satisfies: **G2** (transport-invisible handlers), **G3** (hot reload
preserving process state), **G4** (the full builtin set — routing, request
handling, persistence, reactive state, auth, files, tasks, schedules,
channels, logging), **G5** (no registry, already true by construction but
unproven at runtime until something runs), **C8** (`stack`-as-lifecycle
execution semantics), **E021** (route conflict detection), and the
remainder of the **D3** inventory (runtime faults correctly limited to
division-by-zero and `!` on `none`, per reference §6).

Proof: **T-G** conformance, including the same-handler-over-HTTP-vs-
WebSocket identity test required by G2 (reference §9.2) — the test that
proves transport really is invisible, not merely documented as such.

## 3. Refactor commands: rename, rekind, radius

Satisfies: **E1** (refactors as commands, not text edits), **E2** (no stale
reference survives a completed refactor), **E3** (blast radius reported
before applying), **E4** (atomic reversibility, byte-identical roundtrip),
**E5** (refusal, not partial application, when radius can't be computed
fully), **E6** (the command set covers refactoring completely enough that
hand-editing is never the easier path).

Proof: **T-E** — blast-radius correctness against the manifest, absence of
the prior state after a refactor (exhaustive search per E2), roundtrip
byte-identity (forward then back), and refusal-on-incomplete-radius as its
own explicit test case, not just an absence of crashes.

Note per ADR-0007: `stored` properties are keyed by full dotted name at
persistence time, so `rename` on a `stored` property carries real data
migration weight that `rename` on anything else doesn't — T-E should cover
that case explicitly when this lands, not treat it as a variant of the
ordinary rename path.

## 4. `ashlar fmt`

Canonical formatting: two-space indentation, `"` over `'` for text
literals, and whatever else reference §1 designates as the formatter's job
rather than the parser's. Currently there is no proof obligation beyond
"produces the canonical form reference §1 describes" and idempotence
(formatting already-formatted source changes nothing) — a dedicated test
suite for `fmt` is itself future work, not yet named.

## 5. F1 incremental-latency benchmark at 1,000 files

Satisfies: **F1** specifically (sub-100ms incremental check at 1,000
source files) as a hard-failing test, per ADR-0007's decision to defer this
benchmark until the front end is complete enough that a number from it
means something. Running it early against today's smaller pipeline would
produce noise, not signal, and would risk being mistaken for a satisfied
requirement.

Proof: a generated 1,000-file fixture project, a single-file touch, and a
hard latency assertion in **T-F** — "hard-failing" meaning the suite fails
the build on a miss, not merely reports a number.

## 6. D5 round-trip metric harness

Satisfies: **D5** (round trips from "agent writes code" to "code is
correct" as the measure of compiler quality; every diagnostic that is a
correction removes a round trip). There is currently no harness that
counts round trips at all — this needs an agent-in-the-loop fixture setup
(deliberately broken source, apply suggested fixes, recheck, count
iterations to a clean compile) before D5 is measurable rather than merely
asserted.

Proof: not yet named. This item is a prerequisite research question
("what does a round trip count as, mechanically?") before a suite can be
written — until that question has an answer, D5 stays on this ledger.

## 7. A5 reference-budget audit

Satisfies: **A5** (no feature costs reference budget disproportionate to
its value) as an ongoing, re-runnable check rather than a one-time
judgment made when each construct was added. Needs a per-construct byte
accounting of reference/ashlar.md — how many of the (currently) under
26,000 of the 40,000-byte budget (T-A1) each documented construct consumes
— so that a future addition's cost is visible before it's paid, and so
that A5's "5% of budget must be worth 5% of the language" comparison has
actual numbers behind it instead of being argued from memory each time.

Proof: a script or suite that maps reference sections to byte ranges and
reports the distribution; the pass/fail criterion (what counts as
"disproportionate") is itself still to be defined and belongs with this
item, not assumed.

## 8. T-A3 surface findings: design decisions owed

Satisfies: **A3/A4** follow-through. The first cold-read gate run
(`suites/t_a3/results/2026-07-22-sonnet.md`) produced two genuine surface
bugs and two prior-import hazards that need design decisions, not code:

- **F1**: a restated `stack`/`pipe` kind on a derived layer reads as
  *override*, not chain-participation — the one actively-wrong reading of
  shown semantics that no compile error can catch. Candidate directions: a
  surface marker for "runs in addition to base," or accepting that this is
  reference-carried and measuring it in the reference-in-context suite.
- **F2**: `{Shape}` map syntax reads as a set literal. Candidate: key-
  explicit map shapes (`{text: Shape}`), which would touch the reference,
  parser, corpus, and examples — a coordinated revision, priced against
  the A1 budget before adoption.
- **F3/F4**: index-yields-optional and the determinism guarantees (map
  iteration order, `every` validation, foreign-call check timing)
  contradict mainstream priors silently; they fail safe via downstream
  checking but must stay prominent in the reference.

Proof: a dated ADR accepting or rejecting each finding, then a re-run of
the gate against the recalibrated rubrics (PROTOCOL.md revision of
2026-07-22).
