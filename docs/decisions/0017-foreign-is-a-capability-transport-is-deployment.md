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
