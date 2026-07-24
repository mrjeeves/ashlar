# ADR-0014: Databases — named in source, bound in deployment; kept off the loop's blocking path

Date: 2026-07-23

Status: proposed

## Context

Two asks, one decision chain: Ashlar programs should read and write real
databases (SQLite and PostgreSQL to start), and a program's durable state
should live *in* a file or database rather than the process's
`.ashlar-state.json` blob (ADR-0007, ADR-0013). Three constraints from the
vision bound every answer:

- **B5 — no locations in source.** No path, URL, or version in a `.ash`
  file. A connection string is a location.
- **Names are the only binding mechanism**, and refactoring is first-class
  (E1–E6): a SQL string embedded in source is name-blind — `rename` cannot
  reach into it, blast radius cannot be computed, and it is exactly the
  stringly-typed freedom ADR-0012 rejects.
- **The single event loop is `!Send`** (`http.rs`): one owner of all
  reactive state, no locks. It serves tens of thousands of connections per
  core *as long as nothing blocks it*. A network database is the first
  builtin that could block it — and the answer is not to thread around a
  blocking call (Node's problem, imported) but to keep the call off the
  loop's hot path, using the non-blocking multiplexed I/O the loop is
  already good at.

The question the design must answer plainly: if source carries neither the
location nor the query, how does a program reach a specific database and
specific data — without either blocking the loop or importing a thread pool
to dodge the block?

## Decision

### 1. Location: named in source, bound in deployment

A store is **named** in source and **bound** outside it — the pattern
`foreign` (§9.10), `style` (ADR-0010), and `files` (§9.8) already use.
Source declares the durability of intent; the manifest and the deployment
record where that lives. The connection string is deployment configuration
(an environment variable or a config file the manifest points at), never a
`.ash` literal. Moving a program from an on-disk SQLite file to a shared
Postgres is a **deployment change with zero source diff** — the same binary
runs on a laptop and in production. This is B5 working as designed: "the
build is state, the code is intent."

### 2. Query: the collection is the table; the Shape is the schema

Ashlar already has a name-keyed, shape-typed collection —
`stored X: {text: Shape}`. A database table *is* exactly that, so no query
string is needed for the common case: the language's existing operators are
the access path.

| you already write | on a DB-backed store it means |
|---|---|
| `people[id]` | select the row keyed `id` |
| `put(people, id, p)` | upsert the row |
| `drop(people, id)` | delete the row |
| `for k, v in people`, `keys(people)` | scan |
| `filter(people, f)`, `find(people, f)` | v1: over the in-memory copy; later: predicate pushed to a `WHERE` |

The table name is the property's full dotted name; the columns are the
Shape's fields. A field rename is already a refactor with computable blast
radius (ADR-0007/0009 stored-key migration), so **schema migration is
derivable**, not a hand-written `ALTER TABLE`. No SQL appears in `.ash`
source because the shape is the schema and the operators are the query.

Arbitrary SQL — a legacy schema Ashlar does not own, a reporting join —
stays where every foreign language stays: **in the shim, across the
`foreign` boundary, never in source.** `foreign recent: (room: text) ->
data` names the function in Ashlar; its SQL lives in `foreign/<space>.so`,
exactly as a stylesheet's rules live in `assets/*.css`. SQL is the
persistence peer of CSS: named in Ashlar, defined outside it.

### 3. The blocking boundary: non-blocking database I/O on the loop, no new threads

The loop's strength is non-blocking multiplexed I/O — it already juggles
many HTTP and WebSocket sockets without blocking on any one. A database
connection is one more such socket, so the resolution needs no threads and
no change to the `!Send` evaluator:

- **Reads never touch the network in a handler.** A DB-backed
  `stored X: {text: Shape}` keeps its working set in memory (loaded at boot,
  before serving), so `people[id]` and `filter(people, f)` read memory
  exactly as today — and reactivity still observes those reads and
  re-renders. Keeping the working set in memory is not a compromise; it is
  what preserves Ashlar's reactive core, which is built on tracking
  in-memory reads. Reads from the database on every access would kill it.
- **Writes update memory now, persist off the hot path.** `put(people, id,
  p)` rebinds the in-memory value immediately (reactivity fires at once) and
  the change is persisted out of the handler's way: for **SQLite**, a
  synchronous sub-millisecond local write (durable immediately — no
  different from the loop's existing small file reads); for **Postgres**, a
  non-blocking write on the connection socket the loop already polls,
  drained as the socket accepts it — the same queue-and-drain shape as the
  WebSocket out-buffer.
- **The loop never blocks on the network.** The Postgres client is a
  non-blocking state machine (partial reads reassembled, like the
  hand-rolled HTTP parser), not a blocking call. Only the one-time boot load
  and a rare reconnect handshake may connect synchronously (pre-serve, like
  loading the state file); steady-state query and result traffic is
  non-blocking.

So v1 adds **no OS threads, no `Rc`→`Arc`, no state token, no new
`unsafe`** — the single-writer loop stays exactly as simple as it is today,
and "the runtime schedules around [blocking]" (§9.10) is honored by keeping
network I/O non-blocking rather than by threading around a blocking call.
Threads remain a **named future frontier** for one specific case only —
synchronous read-through of a table too large to hold in memory — and even
then would be confined to the query boundary, never the reactive evaluator.

### 4. `stored` gets a backend; SQLite and Postgres are two bindings

- Default backend: today's JSON file (unchanged, zero-config, the laptop
  default).
- **SQLite** via the `foreign` boundary — proven end-to-end already: a
  std-only Rust `cdylib` links the system `libsqlite3` over the C ABI (a
  system library, not a crate — the boundary's premise) and speaks the
  runtime's `char* f(const char*)` JSON ABI. Local and sub-millisecond, so
  writes are synchronous write-through with immediate durability.
- **Postgres** via a **hand-rolled, non-blocking wire client** in-tree
  (zero crates, the HTTP/WebSocket/SHA precedent): startup and
  SASL/SCRAM-SHA-256 auth (adding SHA-256 + HMAC-SHA-256 — bounded,
  testable primitives in the class we already hand-roll, not the TLS stack
  ADR-0013 forbids), then simple/extended query, `RowDescription`/`DataRow`
  decode, and type mapping — all as a non-blocking state machine on the
  loop.

The working set is the in-memory reactive truth; the database is its
durable, shareable home — **boot-load, then persist on change.** SQLite
persists synchronously (immediate durability); Postgres persists
write-behind on the non-blocking socket, so a crash can lose only writes
still in the short flush window — a bounded, named limit. Synchronous
cross-network durability (and bigger-than-RAM read-through) is recorded as
the later frontier, not claimed for v1.

### 5. Horizontal scale: many single-threaded loops, one database

With state in a shared database, N Ashlar processes run behind the proxy
(ADR-0013's edge), **each still exactly today's pure single-threaded
loop**, sharing nothing in-process. `stored` and sessions live in the DB;
cross-process `synced`/channels ride Postgres `LISTEN`/`NOTIFY` — which also
keeps each process's in-memory working set coherent by invalidating on
change. Scale is by process count, not by in-process threads. Gated behind
its own proof and its own ADR before build.

## Staged build (each stage lands green, with its proof)

1. **SQLite example** — the proven `cdylib` shim over `foreign`; the first
   example to exercise `foreign` at all; a driven T-Examples test asserting
   data survives in the real SQLite file. Ships today, no runtime change.
2. **`stored` backend + non-blocking Postgres client** — the backend seam
   on `stored` (boot-load + persist-on-change), SQLite synchronous,
   Postgres a non-blocking wire state machine on the loop. Proof: a T-G test
   starts a throwaway PG 16 cluster (installed on the CI image), drives the
   client, asserts round-trip, durability across restart, and that a
   deliberately slow server response never stalls other connections.
3. **Scale** — multi-process + `LISTEN`/`NOTIFY` cache coherence. Proof: two
   processes, one DB, a change in one appears in the other. Its own ADR
   first.

## Consequences

- **No new threads, no `Rc`→`Arc`, no new `unsafe`.** The single-writer loop
  is untouched; the database is reached the way the loop already reaches
  every peer — non-blocking. This is the direct answer to "are we remaking
  Node's threading": we lean on the event loop's real strength (multiplexed
  non-blocking I/O) and keep blocking calls off the hot path entirely.
- **No requirement changes.** G4 already names persistence a builtin; this
  implements it. G1 (zero crates) holds — SQLite is a system library over
  the C ABI, Postgres is hand-rolled. G2/G3 are preserved (transport stays
  invisible; hot reload still carries state). The hierarchy stays honest:
  an ADR plus reference sync on implement, not a requirements revision.
- **Honest costs.** v1's working set must fit in memory (cache-everything —
  the same shape as today's in-memory `stored`, now durable and shareable);
  and Postgres durability is write-behind with a bounded loss window.
  Synchronous cross-network durability and bigger-than-RAM read-through are
  the named later frontier — the only place threads would ever return, and
  even then confined to the query boundary.
- **Sync duties on implement**: reference §9.3 (the `stored` backend and its
  deployment binding) and §9.10 (non-blocking foreign/DB scheduling made
  true), a diagnostics row or two (backend bind failure, shape/column
  mismatch), roadmap items, and the tests above.
- **Security mirrors ADR-0013.** The hand-rolled PG client speaks the
  plaintext protocol; the database link is secured at the network layer (a
  unix socket, a private network, or a TLS-terminating proxy such as
  pgbouncer), not by a second in-binary TLS stack. Named as a bounded
  limitation, not hidden.
