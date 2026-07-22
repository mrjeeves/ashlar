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

## 4. `ashlar fmt` — DONE 2026-07-22 (moves off this page next revision)

Delivered as `crates/ashlar/src/fmt.rs` + the `fmt [--check]` CLI
command: two-space indent, `"` quotes, one spacing convention, comment
and blank-line preservation, precedence-faithful re-parenthesization,
and a refusal to rewrite any file with lex/parse diagnostics. Proof:
three properties enforced over the whole t_a3 corpus and every reference
```ash block — formatting preserves the AST (spans aside), formatting is
idempotent, and comments survive by count — plus targeted tests for
quotes, spacing, trailing comments, and multiline literals. E021 (route
conflicts, reference §9.2's "two routes matching one path" rule) also
landed in this increment, closing the last reserved diagnostic id.

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

## 7. A5 reference-budget audit — DONE 2026-07-22 (moves off this page next revision)

Delivered as `crates/ashlar/tests/t_a5.rs`. The criterion, decided with
the suite: sections are measured at the finest heading level (`###`
where present), because A5 governs constructs and a chapter is many; no
single section may exceed 20% of the bytes actually used. Current state:
26,229 of 40,000 bytes; the largest construct is merge kinds (7.5%, plus
3.9% for stack/pipe detail) — the heart of the language, priced
accordingly; every runtime builtin sits between 1.3% and 4.7%. The
distribution prints on every run so future additions are argued from
data.

## 8. T-A3 surface findings — DONE 2026-07-22 (moves off this page next revision)

Resolved by ADR-0008 and validated by gate run 2
(`suites/t_a3/results/2026-07-22-sonnet-run2.md`, 23/24 PASS): F2 fixed
by the `{text: Shape}` syntax change and confirmed by measurement
(0/4-with-wrong-claim → 4/4 clean); F1 kept with the E005 write-time
guardrail named and fixture-pinned, its cold-read residual reproduced
and recorded; F3 converted to a compile-time correction by the shape
checker; F4 accepted as reference-carried. The remaining open thread is
recorded inside the run-2 results file: the reader-suggested explicit
composition marker, if the F1 residual is ever judged worth closing.
