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
  proven by T-G's runtime conformance tests (G2 byte-identity, G3 hot
  reload, multiplexed sockets, cross-client reactivity, foreign binding
  with runtime shape faults). Its residual list emptied in increment 8;
  the conformance pass then closed §9.5's instance lifecycle (start
  stacks on mount, page-scoped unmount with subscription cleanup),
  §9.1's root selection (`run <part>`, candidates listed when
  ambiguous), and `fix <id>`. Hardened 2026-07-23 against real browser
  socket behavior: requests assemble and responses drain without ever
  blocking the loop (a speculative preconnect socket that sends nothing
  once froze the whole runtime), outbound WebSocket frames queue per
  connection and shed peers by time-without-progress — never burst
  size — and an oversized body gets a 413 naming the limit instead of a
  reset. All of it pinned in T-G with hostile-socket tests. The view
  model was made AI-first the same day (ADR-0011): a view instance is
  its own root element (no wrapper breaking a parent's CSS layout), and
  nested views reconcile by position so per-instance state and
  subscriptions survive re-renders and `start`/`stop` fire once — the
  fix a flagship of parts-in-parts demanded.
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
- **Showcase corpus** — thirteen complete projects, crowned by
  `commons`: a full team chat (auth, rooms, DMs, live messaging,
  presence-by-lifecycle, unread counts, plus moderation and mentions as
  independently owned layers) that exercises the whole language as one
  product, styled by a named sheet (ADR-0010). `ledger` is the first to
  exercise the `foreign` boundary for real: its datastore is a genuine
  SQLite database file, reached through a std-only cdylib shim that links
  the system libsqlite3 — the SQL lives outside Ashlar, the way CSS does.
  T-Examples compiles, format-checks, serves, and drives every project —
  commons and ledger included — over its real HTTP/WebSocket surface (the
  ledger driving test builds its shim and skips loudly where libsqlite3 is
  absent, since a SQLite integration cannot be tested without SQLite).
- **Deployment posture** — the binary is an origin; TLS and HTTP/2/3 are
  terminated at a reverse proxy (ADR-0013). The origin carries only the
  small correct pieces to sit behind one: `stored` state flushes
  atomically (temp + rename, so a crash never truncates it), and the
  session cookie is `HttpOnly` + `SameSite=Lax`, gaining `Secure` when
  `X-Forwarded-Proto` reports TLS. Both pinned in T-G.

What remains is not debt but doctrine, named where it lives:
`Unknown`-permissiveness for what the checker cannot prove (no false
positives, check.rs module docs), `move`'s byte-identity class
(ADR-0009), and the open cold-read thread recorded in the run-2 results
file. (The once-weak v1 password hash is gone: v2 is salted, iterated
PBKDF2, and v1 hashes upgrade transparently on login.) New requirements
enter here as new numbered items; none are open today.

One proposed trajectory is on the table but not yet accepted: **ADR-0014**
(status: proposed) sketches the data layer beyond the `foreign` shim the
`ledger` example already demonstrates — a database backend for `stored`
(the collection is the table, the Shape is the schema, the location bound
in deployment not source), a hand-rolled non-blocking Postgres client that
never blocks the single loop, and horizontal scale by process count. Its
Stage 1 (the SQLite-over-`foreign` example) is delivered; Stages 2–4 wait
on a design decision before any runtime code, and will land here as
tested items if accepted.

Delivered 2026-07-24 — **ADR-0015** re-cut the storage taxonomy along its
two real axes. `synced` is retired: the runtime never gave it any behavior
`state` lacks, since no-client-code makes cross-client reactivity
universal. `owned` is added, a per-user scope modifier on `state`/`stored`
— each authenticated user's own value, isolated by construction, so the
manual `[req.user.id]` keying that invites IDOR disappears. It fails loud
where there is no user (an anonymous request, a scheduled task, `spawn`, or
`start` stack): a runtime fault, never a silently shared value. The word
was chosen by a T-A3 cold read (`owned`/`personal`/`user` all read per-user
3/3; `private` misread as OOP access-control). Shipped with the runtime
scoping, per-user persistence keyed by the stable account id, `E029`
(`owned` needs a storage word), the `ticker` rename, the `locker` example
(two users, isolated and persisted, driven by the suite), a T-G fault
proof, and the reference/G4 rewrite. One refinement stays named: catching
the no-user case at COMPILE time in provably user-less contexts
(task/boot/`spawn`) — the runtime fault already secures correctness; a
static check would only move the failure earlier.
