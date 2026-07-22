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

## 2. Evaluator and runtime — v1 DONE 2026-07-22 (honest gaps below)

Delivered as `crates/ashlar/src/eval.rs` + `http.rs` + `ashlar run`:
immutable value model, state store keyed by full dotted name, stack/pipe
execution (C8), the std builtin set at runtime, exactly two runtime
faults (D3), a hand-rolled zero-dependency HTTP/1.1 + RFC 6455 server on
a single-threaded event loop, `stored` persistence to
`.ashlar-state.json`, `allow` guards, `fail` statuses, JSON/text/HTML
rendering, `every`/`run` schedules, and hot reload carrying state over
by full name (G3). Proof: **T-G**, five conformance tests including the
G2 identity test — the same handler produces byte-identical results over
HTTP and WebSocket envelopes.

2026-07-22, second pass — the live view protocol (§9.4) is delivered:
`el(PartName, fields)` instantiates per use with per-instance `state`;
views render server-side with a ~20-line zero-dependency browser shim;
events round-trip over the socket to run handlers in their instance;
every instance whose state changed re-renders and patches in place.
Session auth (§9.6: signup/login/logout with cookies, persisted
accounts, `req.user`), static file serving with a traversal guard
(§9.8), and queued `spawn` (§9.7) also landed. T-G is 9 conformance
tests. Still open: foreign-function binding at run time (a call
faults), and cross-instance reactivity for `synced` (a change patches
instances that assigned it, not yet every view that read it).

## 3. Refactor commands — v1 DONE 2026-07-22 (scope below)

Delivered as `crates/ashlar/src/refactor.rs` + `ashlar rename` /
`ashlar rekind` (`--plan` prints the radius without applying). Every
command computes its complete blast radius first and reports it (E3);
edits apply to an in-memory copy that is re-checked, and any diagnostic
rolls the whole refactor back (E4/E5 — nothing partial ever reaches
disk); T-E proves forward-then-back byte-identity for part renames,
property renames (including `stack`-returned map keys, which merge onto
state by name), and rekind. Refusals are total and reasoned: broken
projects, data-shape fields (constructing literals not yet tracked),
multi-line dotted chains, unknown targets, and post-verify failures.

Remaining scope (E6 is not yet fully met): data-shape field rename
awaits literal tracking; `move` (part between spaces) and space rename
await `use`-graph rewriting; `stored` renames migrate no persisted data
yet (ADR-0007's note stands — the state file keys by full name, so a
rename orphans old rows until a migration step exists).
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

## 5. F1 incremental-latency benchmark — DONE 2026-07-22

Delivered as `check_sources_incremental` (per-file parse cache keyed by
content hash; global phases always rerun) plus `tests/t_f1.rs`: a
1,000-file project, one changed file, a hard sub-100ms assertion in
release builds (debug builds measure and report). Landing the gate
surfaced and fixed the real bottleneck: the resolver and checker built
per-space visibility by scanning every part for every space — an index
by home space cut resolve 67→20ms and check 25→8ms. Current numbers:
full pass 47ms, incremental 40ms. Headroom exists (the global phases
could go incremental too) but the requirement is met as written.

## 6. D5 round-trip metric harness — DONE 2026-07-22

The open question — what counts as a round trip, mechanically? — is
answered in `tests/t_d5.rs`: **one check → apply-machine-edits cycle.**
The harness runs that loop over every T-A4 fixture whose diagnostics
carry machine edits, asserts convergence within 3 rounds, and gates the
mean at ≤1.5. Current state: **11 machine-fixable fixtures, mean
rounds-to-clean 1.00** — every machine-visible error in the corpus is
one round trip from clean, which is D5's ideal. Judgment-required
diagnostics (notes without edits) are D1's territory and excluded.

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
