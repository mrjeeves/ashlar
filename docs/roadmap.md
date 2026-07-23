# Roadmap

An honest "not yet" ledger. Each item names the requirements it will
satisfy and the test that will prove it. A "planned" row anywhere in the
suite tree — including `suites/coverage.md` if and when one exists — is a
debt-ledger entry, not coverage. Nothing on this page is done; when an item
is done, it moves off this page and its test starts running for real,
because a requirement with no passing test is not a satisfied requirement
(T-META).

## The ledger is empty — 2026-07-22

Every item this page has carried is delivered, tested, and moved off:

- **Shape checker** — `check.rs`, proven by its unit suites, T-A4
  fixtures 31–38, and every reference example checking clean (T-A2).
  The D3 inventory it left behind (stack/pipe cross-layer agreement,
  route capture rules, deeper inference for unannotated recursion)
  closed in increment 9: pipe layers must agree in parameter and return
  shape, stack return literals must name state properties with fitting
  values, captures must be legal names bound once, and `-> ?` returns
  refine from concrete branches so recursive callers check (ADR-0009).
- **Evaluator and runtime** — `eval.rs` + `http.rs` + `ashlar run`,
  proven by T-G's 15 conformance tests (G2 byte-identity, G3 hot
  reload, multiplexed sockets, cross-client reactivity, foreign binding
  with runtime shape faults). Its residual list emptied in increment 8;
  the conformance pass then closed §9.5's instance lifecycle (start
  stacks on mount, page-scoped unmount with subscription cleanup),
  §9.1's root selection (`run <part>`, candidates listed when
  ambiguous), and `fix <id>`.
- **Refactor commands** — `refactor.rs` + `rename`/`rekind`/`move`/
  `radius`, proven by T-E's 13 tests. The E6 residuals closed in
  increment 9: data-shape and view fields rename through the checker's
  field-site index; spaces rename as pure prefix substitution; `move`
  relocates a home declaration with `use`-graph additions and a stated
  E4 class (ADR-0009); `stored` keys migrate with their names, closing
  ADR-0007's orphaned-rows note. `vendor` landed with
  refuse-before-copy and roll-back-after semantics.
- **`ashlar fmt`** — comment-preserving canonical formatter with
  AST-preservation, idempotence, and comment-count properties enforced
  over the whole corpus.
- **F1 incremental latency** — hard sub-100ms release gate at 1,000
  files; currently 40ms incremental.
- **D5 round-trip metric** — one check → apply-machine-edits cycle;
  mean rounds-to-clean 1.00 over every machine-fixable fixture.
- **A5 reference budget** — 26,352 of 40,000 bytes, largest construct
  7.5%, distribution printed on every run.
- **T-A3 surface findings** — resolved by ADR-0008, validated by gate
  run 2 (23/24 cold-read PASS).
- **Showcase corpus** — ten complete projects now include typed
  multi-space policy composition and background work driving a live
  view. T-Examples compiles, format-checks, serves, and drives both
  additions over their real HTTP/WebSocket surfaces.

What remains is not debt but doctrine, named where it lives:
`Unknown`-permissiveness for what the checker cannot prove (no false
positives, check.rs module docs), `move`'s byte-identity class
(ADR-0009), and the open cold-read thread recorded in the run-2 results
file. (The once-weak v1 password hash is gone: v2 is salted, iterated
PBKDF2, and v1 hashes upgrade transparently on login.) New requirements
enter here as new numbered items; none are open today.
