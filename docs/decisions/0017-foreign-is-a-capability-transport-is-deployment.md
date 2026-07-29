# ADR-0017: `foreign` declares a capability; the transport is a deployment fact

**Amended by [ADR-0019](0019-a3-run3-findings-owned-and-reactive-annotation.md):**
the reactive annotation referred to below as `reads`/`writes` is now spelled
`watches`/`updates`. Every claim about transports composing with it still holds.

Date: 2026-07-24

Status: accepted

## Context

The `foreign` boundary (§9.10) worked, but it demanded that the foreign world
rebuild itself in Ashlar's image. To call anything you had to satisfy four
rules at once:

1. **Our path** — the library had to live at `foreign/<space>.so`, named after
   the Ashlar space, and nothing else was looked for (not even `.dylib`, so
   macOS was unreachable).
2. **Our symbol names** — the exported symbol had to be spelled exactly like
   the Ashlar declaration, so an existing library could never be bound
   directly; `sqlite3_open` could not be reached as `open`.
3. **Our ABI** — every function had to be reshaped to
   `char* name(const char* args_json)`.
4. **Our toolchain, effectively** — since (1)–(3) are satisfiable only by
   compiling a new artifact, every integration meant writing and building
   glue in another language.

The `ledger` example is the evidence: reaching SQLite — a library that already
exists, already works, and is already installed — cost ~150 lines of Rust
shim. Ashlar was not binding to foreign systems; it was requiring them to be
re-authored. That is the opposite of a boundary.

Notice what was *not* the problem: JSON at the seam. That is doing real work —
it is why shape-checking is uniform and why the boundary is language-neutral
in principle. The mistake was bolting a universal data format onto a
maximally non-universal delivery mechanism.

## Decision

> **A `foreign` declaration names a capability — a name and a shape. *How*
> that capability is reached is a deployment fact, not a language fact.**

This is not a new principle. It is the principle this repository already
applies everywhere else, finally applied to the mechanism instead of only the
path: `style = "commons"` names a sheet the build locates (ADR-0010), `port`
is overridden by `--port` at run time (ADR-0016), and ADR-0014 binds a
database location in deployment. `foreign` was the last place where a name in
source still dragged a hardcoded transport behind it.

**`.ash` source does not change at all.** Every existing declaration keeps
working, and the reactive `reads`/`writes` annotation composes with all of the
below unchanged.

### 1. A binding file, deployment-side

An optional `foreign.json` at the project root (override the location with
`ASHLAR_FOREIGN`) says how each space is reached:

```json
{
  "ledger.store": { "via": "native", "library": "foreign/ledger.store.so" },
  "vision":       { "via": "worker", "run": ["python3", "foreign/vision.py"] },
  "billing":      { "via": "http",   "url": "http://127.0.0.1:9000/rpc" },
  "geo":          { "via": "native", "library": "/usr/lib/libgeo.so.3",
                    "symbols": { "lookup": "geo_lookup_v2" } }
}
```

It is JSON because the runtime already contains a JSON parser; a new file
format would be new surface for nothing.

**This is an override, not a fallback.** With no entry (or no file at all), a
space resolves by the *derivation rule* that already exists: the native
library at `foreign/<space>`, now probing `.so`, `.dylib`, and `.dll`. One
derived default, one explicit override, and the manifest records whichever
won — exactly the relationship `--port` has to `port`. Making the file
mandatory was considered and rejected: it breaks every existing project and
forces ceremony onto the simplest case.

The file keys bindings by SPACE NAME, and that has a consequence worth stating
plainly, because the first cut of this ADR missed it: `foreign.json` is the one
file outside `.ash` carrying a name the compiler reasons about. Every system
that governs names therefore has to see it, or the language quietly loses a
guarantee it advertises. Three did not, and each failure was real:

- **`rename` left the key behind.** The sources rewrote, the program still
  checked clean, and the space fell back to the derived native path — a stale
  reference to the prior state (E2) that surfaced only when a request reached
  the boundary. The key and all three probed library extensions are now radius
  (E3), the CLI carries them, and reversing the rename restores the file
  byte-for-byte (E4).
- **`check` ignored the file.** A mistyped key resolved to nothing and said
  nothing, which B3 forbids for every other name in the language. It is now
  `E001`, with the nearest space named in the correction; an unparseable file
  is reported rather than silently becoming the derived default. A key whose
  space exists but declares no `foreign` stays silent — inert is not wrong, and
  guessing there would be a false positive.
- **The manifest recorded the derivation rule instead of the resolution.** A
  worker-backed space was written down as a native library path that did not
  exist, which makes the derived state a fiction about the running program.
  It now records the transport that won and what it names.

The lesson generalizes past this ADR: adding a name-bearing file is not
finished when the runtime can read it. It is finished when the refactor
commands, the checker, and the manifest can all see it.

### 2. Three transports

- **`native`** — `dlopen` a shared library and call the C ABI. Now with
  arbitrary library paths (including system libraries), **symbol aliases** so
  an Ashlar name can bind a differently-spelled export, and platform probing.
- **`worker`** — a long-lived co-process speaking **JSON Lines** over
  stdin/stdout. This is the order-of-magnitude change: a worker in any
  language is about eight lines, needs no compiler, no C ABI, no symbol
  naming, and no shared library.
- **`http`** — POST the same JSON envelope to a URL, for a capability that is
  already a service.

The worker protocol, in full:

```
→ {"call":"record","args":["ada","coffee",4.5]}
← {"ok":true}
← {"error":"database is locked"}
```

### 3. One result convention, every transport

- an object with exactly one key `error` (a text) is a **fault** carrying that
  message;
- an object with exactly one key `ok` yields that value;
- **any other value is the result itself.**

The third rule keeps the simple case free of ceremony (a native shim may
still return bare JSON, as `ledger`'s does), the first turns a foreign failure
into a diagnosable fault instead of "returned malformed JSON", and the second
is the escape hatch for the one ambiguous case — a result that genuinely is
an object whose only key is `error`.

### 4. Memory and lifecycle

The C ABI gains an **optional `ashlar_free(char*)`** export. If a library
exports it, the runtime calls it with the returned pointer; if not, behavior
is unchanged. Today every native call leaks its result buffer, and there was
no protocol by which a shim could say otherwise.

A worker is spawned lazily on first call and kept alive. If it dies, the call
faults loudly and the *next* call respawns it. That is process lifecycle, not
failover.

### 5b. One name whose default is the machine's, not the project's (2026-07-27)

The derivation rule answers "where does this capability live" with a path
*inside the project* — `foreign/<space>` — which is right for a capability the
project supplies and wrong for one the machine already runs. The mesh node is
the second kind: installed once, shared by every program on the box, exactly
like the proxy ADR-0013 puts in front of the origin. Deriving `mesh` to
`foreign/mesh.so` would have meant every project that wanted a roster shipped a
shim to a daemon it did not own.

So exactly one space name — `mesh` — derives to a co-process instead of a
library. **And the co-process is this toolchain.**

Two wrong answers were built first, and both are worth recording because each
looked reasonable while it lasted.

**The first put the adapter in the mesh's own repositories**, adding an
Ashlar-shaped subcommand to each so that `mesh` derived to `myownmesh ashlar`
and `mesh.sites` to `allmystuff-ashlar`. That is precisely the failure this ADR
exists to end, wearing a different face: the `ledger` shim required a foreign
library to be re-authored for us, and this required a foreign *product* to be.
Every property that made it look right — the adapter lives with the thing it
adapts, it moves when that thing moves — is bought with the one cost a boundary
may not carry, which is that the integration does not exist until somebody else
ships a change for you. The node already has a client: a control socket
carrying JSON, driven by its own GUI and by every app on that stack. Being that
client is what a boundary means.

**The second split the capability across two sockets** — the roster from the
mesh daemon directly, the sites from the node — which meant two wire protocols
and, since one of them was reached a way `std` could not open on Windows, a
platform where the whole feature was a message explaining its own absence. Both
were self-inflicted. The node forwards the roster ops (`mesh_identity`,
`mesh_peers`, `mesh_networks`, `mesh_network_add`) to the daemon it already
supervises, so one socket answers everything; and a Windows named pipe opens
with `std::fs::OpenOptions`, so the second platform costs four lines and no
dependency. A capability that works on one operating system is not a
capability, and "the runtime is zero-dependency" was doing the arguing for a
conclusion that was never true.

### 5c. The floor was too high (2026-07-29)

Three transports, and the cheapest way to reach SQLite was a **165-line C-ABI
shim** plus a Rust toolchain on the machine that runs the site. That is what
`examples/ledger` costs, and the showcase script has a branch that skips the
example when `rustc` is missing. A capability priced like that is one an author
does not reach for — which makes the boundary a claim rather than a door,
whatever the ADR says about it.

So a fourth transport: **`command`**. An ordinary program, run once per call,
argv in and stdout out. No ABI, no envelope, no co-process protocol, nothing to
write on the far side. `run` names the program, `args` maps an Ashlar name to
the argv items that select it — the same relationship `symbols` has to an
export — defaulting to the name itself, so a tool shaped like `git status`
binds with nothing written at all. Output is JSON if it parses and text
otherwise; a non-zero exit is a fault carrying stderr, because that is where
programs put the reason.

What it does not fix is worth stating: `sqlite3`'s CLI has no parameter
binding, so a WRITE still needs something that can take values — a script of
its own, which `command` also reaches, at fifteen lines in any language instead
of a hundred and sixty-five in C ABI. The floor moved; it did not vanish.

`check` proves a command by LOOKING: a path is a file, a bare name is on
`PATH`. There is no side-effect-free invocation to probe with, because the
arguments belong to the program — `sqlite3 --version` is safe and
`rm --version` is a guess about somebody else's CLI.

**A worker may speak first.** The envelope was strictly request/response, so
`watches`/`updates` could only fire on a call — and a collection that changes
because something OUTSIDE the program changed has no call to fire on. The mesh
roster made that concrete: the library carried a three-second schedule asking
"did it move", which is late by up to three seconds, wrong on a slow answer,
and pure waste on a quiet mesh. The node had been streaming presence to its own
front end the whole time.

So a worker may print `{"changed": "<Shape>"}` at any moment, unasked: the same
dependency edge `updates` makes, with no call under it. The runtime reads each
worker on its own thread — reading only inside a call would leave a push in the
pipe until the next one, which is a poll wearing a push's clothes — and the
server loop, already awake for sockets, dirties the collection's readers. A
worker that never pushes is unaffected, which is why this is an addition to the
envelope and not a version of it. The cost is one thread per worker and one
queue; the alternative was every reactive co-process inventing its own schedule.

### 5d. What the room turned out to cost (2026-07-29)

Four things were wrong on the way to a file crossing between two machines, and
every one of them failed quietly. They are recorded because each is a trap the
next reader would fall into identically.

**A route has a direction and its source is a handle.** `from = <peer>:shared`,
`to = me` — the `:shared` suffix is what marks the lane fetch-only, and the
node checks the fetcher is the route's `to`. Opened the other way it is a route
that exists, reports active, and refuses every request on it.

**A route endpoint must carry the DISPLAY form of a node id** (`pubkey-SUFFIX`).
The roster and room messages carry bare pubkeys; presence carries the display
form. With the bare form the node reports the route ACTIVE, accepts the fetch,
and answers nothing, ever. Proved by running one fetch twice against two real
nodes with one variable changed. Only presence knows the suffix, so `addressable`
asks presence.

**A `*_poll` answers with a raw batch under tag 1, not JSON under tag 0.** The
adapter refused every one of them as a protocol break. It is the same frame the
camera lane uses, so fixing it for files fixed it for video.

**A fetch does not end in a poll queue.** Its chunks stream straight to disk;
the outcome arrives as `allmystuff://file-saved` on the event stream. Waiting
on the queue was a timeout by construction.

And one that was ours alone: the first working version **blocked the server
loop** for the length of a transfer. A foreign call that waits on a network
freezes every page on the machine. A transfer is started and the arrival
pushes — the shape everything else here already had.

**A room's files are not a share.** Two things in that stack look alike and
are not: a `Share` is a durable grant relationship with a *person* who brings a
fleet, minted by an explicit act and revoked by another; a room's Shared Files
lane is a token whose allow-list is the room's members, checked on every fetch
and gone when the offer is restated without it. The node's own code draws the
line — the shared lane is "token-gated, never owner/fleet", and the route
carries exactly one request, `Fetch { req, token }`, with no path browsing and
no writes.

An Ashlar room takes the second and none of the first. Membership IS the
authorization, which is the same sentence as "the mesh id is the invite":
nothing durable is granted to anyone, so nothing has to be revoked, and a
member that leaves the mesh stops being on the allow-list the next time the
offer is stated. Reaching for `share_grant` here would have added a person
model, a grant lifecycle, and a revocation story to a room whose whole
admission rule is already one secret.

**The mesh is the room.** The node's rooms have a host: it mints the id,
states the roster, and admits knocks. That is right when a room is a subset of
a mesh, and wrong for an Ashlar app, whose mesh id is already the invite —
everyone holding it is in. So the room id is derived from the mesh's name,
every member computes the same one, and there is no host to be offline and no
admission step to get wrong. What that costs is a room nobody can be excluded
from without changing the mesh, which is the same sentence as "the id is the
secret" and is why rolling a new one is how a group changes its locks. The
host-based form is still there for a program that wants it; this is the shape
the language's own library ships.

**Being a client constrains what this may write.** A control socket carries the
whole machine, not this program's corner of it. The adapter reads freely and
writes only what the program itself put there — a network it joined, a port it
exposed. A third wrong answer shipped before that rule did: `enter` set the
node's display label from the app's `label` setting, so starting an Ashlar site
renamed its owner's node, for every peer on every mesh that node was on, with
nothing to say it had happened. The setting names the app now — the network it
joins, the site it publishes — and the refusal is by command name, in the one
function every request passes through, so it holds for call sites not yet
written.

**An absent node is a fact about a machine, not a fault in a program.** Reads
answer around it, with an empty roster that carries the correction rather than
one that merely looks lonely; a deliberate publish still fails, because
`run --mesh` printed a promise. The build that faulted on every read took the
whole site down on any machine whose node was closed, never installed, or (WSL)
across the kernel boundary — the example did not start at all. Reachability was
already not a build-time fact (§5); the same holds at run time, and only a
caller who asked for the capability is owed an error about it.

The cost that remains is real and worth naming: this repository tracks one wire
protocol it does not own. That is the ordinary cost of being a client. It is
bounded — the ops used are the ones that node's own front end drives, the most
stable surface it has — and it is visible, in one module with the socket named
in its header.

Everything else is unchanged: a `foreign.json` entry overrides the name, `check`
still reports a key naming no space, the manifest still records the transport
that won, and the mechanism is the ordinary worker transport, so the mesh is
reached exactly the way any third-party co-process is. The rule this ADR closed
with holds here too, and cost one more thing to notice: **a name that IS the
binding has no file for a rename to carry.** Renaming a space onto or off this
one silently swaps its transport, with no key to rewrite and no library to move
— so `rename` reports it as radius (E3) even though it changes nothing on disk.

### 5. `ashlar foreign check`

A new toolchain command that verifies every declared foreign name is actually
reachable: dlsym each symbol for `native`, spawn and speak the protocol for
`worker`. It converts "unreachable foreign function" from a fault that
surfaces when a user first hits that route into a build-time diagnostic with a
correction — diagnostics-as-corrections applied to the boundary.

## What was rejected, and why

- **Native C type marshalling in source** (`foreign sqlite3_open: (text, out
  handle) -> number`). The obvious ask, and a trap. It needs a hand-rolled
  calling-convention marshaller — no libffi under G1 — which is
  architecture-specific `unsafe` far beyond the `dlopen` boundary, and it
  drags pointers, out-params, ownership, and nullability into a shape system
  that has none of them. A worker in any language reaches the same libraries
  for a fraction of the cost.
- **WASM as a foreign target.** Attractive for sandboxing and portability; an
  in-tree zero-dependency runtime is enormous. Revisit only with its own ADR.
- **Command templating** (`exec ffmpeg -i {path}`). Invites argument
  injection; a small adapter script is safer and no harder.
- **A `foreign scaffold` code generator.** Considered and dropped: it is new
  toolchain surface with per-language templates to maintain, it duplicates
  what the reference must say anyway, and maintaining Python/Node/Rust idiom
  is precisely the "foreign systems following our conventions" problem this
  ADR exists to end. The eight-line worker lives in reference §9.10 instead,
  where it costs a few hundred bytes and no code.
- **Transport failover.** One name with two sources of truth and silent
  degradation is the quiet-wrong this repository refuses. Unreachable is a
  loud fault.

## Consequences

- **Reach.** A capability can now be backed by a system library, a Python or
  Node process, an existing CLI, or a service — without a compiler and
  without touching a line of `.ash`. `reads`/`writes` reactivity (ADR-0014)
  composes on top, so a Python worker over a database still patches every
  connected client live.
- **The `unsafe` moves.** The whole boundary now lives in `src/foreign.rs`,
  which confines the one `unsafe` (dlopen/dlsym) to the module named for it
  rather than burying it in the evaluator. AGENTS.md's rule is updated to
  match; the invariant is unchanged and easier to check.
- **Blocking is now more visible.** `worker` and `http` calls block the single
  loop — exactly as `native` calls always have — but transports make it much
  easier to bind something slow. This raises the priority of ADR-0014's
  non-blocking work; it does not change today's semantics, and the reference
  states the behavior plainly.
- **Capability surface.** A binding file can exec a process or reach a URL.
  This is the same trust level as loading a `.so` (both run arbitrary code,
  both are deployment-controlled, neither is written in source).
- **No TLS.** The `http` transport is plaintext, consistent with ADR-0013:
  the binary is an origin, TLS is terminated by a proxy. It is meant for a
  co-located sidecar or a local proxy hop, and the reference says so rather
  than implying `https` works.
