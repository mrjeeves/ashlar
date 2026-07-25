# Roadmap

An honest "not yet" ledger. Each open item names the requirements it will
satisfy and the test that will prove it. A "planned" row anywhere in the
suite tree — including `suites/coverage.md` if and when one exists — is a
debt-ledger entry, not coverage. Open items come first; below them is the
dated record of what was delivered and what proves it, because a requirement
with no passing test is not a satisfied requirement (T-META). An empty open
section is a claim, so it is kept honest: an item leaves it only when its
test runs for real.

## Open — 1 item

**A3 fixtures 11 and 25 have not been re-scored since their keywords changed.**
Gate run 3 (2026-07-25, `suites/t_a3/results/2026-07-25-sonnet-run3.md`) scored
**23/25**, clearing the 80% bar, and its two failures were the two constructs
added since run 2. Both have now been respelled per **ADR-0019** — `owned` is
`peruser`, and `reads`/`writes` are `watches`/`updates` — on the strength of a
construct-level candidate cold read in which `peruser` conveyed per-user scope
2/2 (against 0/2 for `owned`, `personal`, `mine`, and `each`) and
`watches`/`updates` conveyed the reactive edge 2/2 (against 0/2 for
`reads`/`writes`).

That candidate read is evidence for a choice, **not a corpus score.** Until run
4 re-scores fixtures `11-peruser` and `25-foreign-reactive` under the full
protocol, the honest claim is 23/25 with two known-bad spellings replaced by
two well-evidenced ones — not 25/25. Requires a fresh reader per
`suites/t_a3/PROTOCOL.md`; deliberately not a CI job.

The methodological finding needs no decision and is already binding:
**cold-read the construct, never the word.** ADR-0015 scored `personal` 3/3 by
testing the bare word; in its actual slot it reads as `private`, the very frame
`private` was rejected for. Any future naming decision tests the syntax a reader
will meet, with its neighbors.

Delivered 2026-07-25 — **the two A3 run-3 findings are fixed in the language**
(ADR-0019). `owned` → `peruser` and `reads`/`writes` → `watches`/`updates`,
across the reserved-word list, tokens, parser, AST and resolved models, the
composer's storage identity, the formatter, the evaluator's per-user scoping and
its two runtime faults, `E029`'s cause and machine fix, reference
§1/§4/§9.3/§9.10, the diagnostics catalog, G4, the `locker` and `ledger`
examples, and the A3 fixtures. `owned`, `reads`, and `writes` are ordinary
identifiers again — `commons` already declares a property called `reads`, which
is now a live proof of it. E029's machine fix still converges in one round
(D2), and all 15 examples check clean and canonically formatted.

Delivered 2026-07-25 — **the binding file is a name-bearing fact, and the
name-governing systems now see it.** An audit of ADR-0017 against the vision
found one root cause with three symptoms: `foreign.json` keys bindings by SPACE
NAME, and none of the three systems that govern names knew the file existed.
`rename` rewrote the sources, left the key behind, and the program still checked
clean while the capability silently fell back to the derived native path — a
stale reference E2 forbids, found only when a request reached the boundary.
`check` ignored a key that named no space, which B3 makes an error for every
other name in the language. And the manifest recorded the derivation rule
instead of the resolution, so a worker-backed space was written down as a native
library that did not exist — a fiction in the file whose whole job is being the
derived truth ("the build is state"). Fixed together: the key and all three
probed library extensions are radius and are carried atomically and reversibly
(E2/E3/E4); an unbound key is `E001` naming the nearest space, and an
unparseable binding file is reported rather than silently becoming the default,
while an inert binding stays silent (no false positives); the manifest records
the transport that won and what it names. Pinned by a T-E end-to-end rename
proof, a T-B resolution test covering all four cases, a T-F manifest test, and
unit tests for the depth-aware key scanner. Two reference sentences and one ADR
paragraph that were false are now true, and D3's silent third category closed:
foreign reachability and `owned`-with-no-user each now state in the reference
WHY they are runtime facts rather than build-time ones.

## Everything else is delivered — 2026-07-22

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
- **A5 reference budget** — under 40,000 bytes with the distribution
  printed on every run; §9.10 is the largest construct as the boundary
  grew (ADR-0017), still far under the 20% per-construct cap.
- **T-A3 surface findings** — the run-1/2 findings resolved by ADR-0008,
  validated by gate run 2 (23/24). Run 3 (2026-07-25, 23/25) cleared the bar
  again and raised two findings, both now fixed by ADR-0019; only the
  re-score is open, at the top of this page.
- **Showcase corpus** — fifteen complete projects, crowned by
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
positives, check.rs module docs) and reversal-as-property rather than
byte-identity-as-law (ADR-0018, generalizing ADR-0009's `move` trade). (The
once-weak v1 password hash is gone: v2 is salted, iterated PBKDF2, and v1
hashes upgrade transparently on login.) New requirements enter here as new
numbered items; the open ones are listed at the top of this page.

One proposed trajectory is partly delivered: **ADR-0014** sketches the data
layer beyond the `foreign` shim the `ledger` example demonstrates — a
database backend for `stored`, a hand-rolled non-blocking Postgres client
that never blocks the single loop, and horizontal scale by process count.
Delivered so far: Stage 1 (the SQLite-over-`foreign` example) and, on
2026-07-24, **reactivity for a foreign store** — a `reads <Shape>` /
`writes <Shape>` annotation on `foreign` that joins the SQL collection to the
§9.3 reactive graph (the collection is the table, the Shape is the schema).
`recent`/`total` `reads Entry`, `record` `writes Entry`, so a write from any
client patches every open `ledger` board live — no new threads, no `stored`
backend, a typo'd collection caught as E001, and a T-Examples test that drives
the cross-client patch. The `stored` database backend, the Postgres client,
and horizontal scale remain proposed, awaiting a design decision before any
further runtime code.

Delivered 2026-07-24 — **ADR-0015** re-cut the storage taxonomy along its
two real axes. `synced` is retired: the runtime never gave it any behavior
`state` lacks, since no-client-code makes cross-client reactivity
universal. `owned` is added, a per-user scope modifier on `state`/`stored`
— each authenticated user's own value, isolated by construction, so the
manual `[req.user.id]` keying that invites IDOR disappears. It fails loud
where there is no user (an anonymous request, a scheduled task, `spawn`, or
`start` stack): a runtime fault, never a silently shared value. The word
was chosen by a T-A3 cold read of the WORD (`owned`/`personal`/`user` all read
per-user 3/3; `private` misread as OOP access-control) — a method ADR-0019 later
showed to be the wrong unit of measurement, respelling the keyword `peruser`. Shipped with the runtime
scoping, per-user persistence keyed by the stable account id, `E029`
(`owned` needs a storage word), the `ticker` rename, the `locker` example
(two users, isolated and persisted, driven by the suite), a T-G fault
proof, and the reference/G4 rewrite. One refinement stays named: catching
the no-user case at COMPILE time in provably user-less contexts
(task/boot/`spawn`) — the runtime fault already secures correctness; a
static check would only move the failure earlier.

Delivered 2026-07-24 — **the foreign boundary meets foreign systems where
they are** (ADR-0017). A `foreign` declaration now names a capability; how it
is reached is a deployment fact. Three transports carry one JSON envelope:
`native` (dlopen, now with arbitrary library paths, **symbol aliases** so an
existing library keeps its own names, `.so`/`.dylib`/`.dll` probing, and an
optional `ashlar_free` that ends the returned-buffer leak), **`worker`** (a
co-process speaking JSON Lines — any language, no compiler, no C ABI, no
shared library), and `http` (plaintext, TLS at a proxy per ADR-0013). An
optional `foreign.json` overrides the derived path per space; with no file,
every existing project resolves exactly as before. A shared result convention
(`{"error": …}` faults, `{"ok": …}` is the escape hatch, a bare value is the
result) makes a foreign failure diagnosable instead of "malformed JSON", and
`ashlar foreign check` proves every declared name reachable — dlsym for
native, protocol handshake for a worker — turning a runtime fault into a
build-time correction. The whole boundary moved to `src/foreign.rs`, which now
confines the workspace's only `unsafe`. Proven by four T-G conformance tests
(one per transport plus alias/free) and the new **`abacus`** example, whose
capability is ten lines of Python. Deliberately rejected and recorded: native
C type marshalling in source, WASM, command templating, a code-generating
`foreign scaffold`, and transport failover.

Delivered 2026-07-24 — **the examples wear a design** (ADR-0016). Every
project now declares a stylesheet in one restrained dark house language (the
`commons` palette, re-declared per project since the runtime serves one sheet
each, bound by `class` name); the four former API-only demos — `diary`,
`press`, `guardrails`, `locker` — grew a small `/` view that shows their idea
in the browser (a live pipe preview, a live policy verdict, a login gate, a
per-user board), each driven by `t_examples`. A top-level `showcase/` runs all
fifteen at once and flips between live frames — a launcher, not a baked
gallery, so the running app stays the only source of truth. Making that work
without editing any example's `port` added `ashlar run --port N` — a
deployment fact bound at run time (B5, reference §9.1/§11), pinned by
`cli::parse` tests — and a viewport `<meta>` now ships in every served head so
the pages are legible on a phone. No language rule changed; the suite stays
green in debug and release.
