# Ashlar

Ashlar is an agent-authored composition language for servers and
interfaces. There is one composable unit, `part` — UI elements, routes,
services, state stores, and data shapes are all parts, composed by the same
mechanism. Names are the only binding mechanism: no file path, argument
position, declaration order, or file location affects what a name refers
to. The build computes composition order and location from source; source
never contains either.

The name is the vision made literal: an ashlar is a stone cut precisely
enough to be laid without mortar — the fit between names *is* the joint.

> **Agents:** your entry point is [`AGENTS.md`](AGENTS.md) — the
> load-bearing contract for working in this repository. This README is
> the human tour; that file is the rules.

## Sixty seconds of Ashlar

A server is a part with a `port`; a route is a part with a `route`
(`examples/hello`):

```ash
space hello

part app {
  port = 8080
}

part greet {
  route = "/"
  handle pipe = (req: std.Request) => "hello from ashlar"
}
```

A UI element is a part with a `view`. It renders server-side; the browser
runs a small transport shim and no program code. Handlers run on the
server, and every view that read a changed state property re-renders and
patches in place — across every connected client (`examples/counter`):

```ash
part tally {
  label: text
  state n: number = 0
  view = () => el("button", { onclick: bump }, [label + ": " + text(n)])
  bump = () => { n = n + 1 }
}
```

And the signature move — extending someone else's part **without editing
their file**, from your own space (`examples/chat`):

```ash
space chat.audit
use chat.data

part chat.data.Store {
  prepare pipe = (body: text) => {
    log.info("prepared", { size: len(body) })
    return body
  }
}
```

Layers flatten in `use` order; five merge kinds (`replace`, `append`,
`deep`, `stack`, `pipe` — plus `reverse`) say exactly how a property
composes, and changing a kind mid-stack is a compile error with the fix
attached.

## Quickstart

```
cargo build --release
target/release/ashlar run examples/chat     # http://127.0.0.1:8080
target/release/ashlar check <dir>           # diagnostics as JSON Lines (--human for prose)
```

Every command in the reference's toolchain table exists and is tested:

| command | effect |
|---|---|
| `ashlar check` | compile; diagnostics as corrections (JSONL, `--human` for prose) |
| `ashlar fix [id]` | apply machine-applicable fixes from the last check |
| `ashlar build` | check, then write the manifest |
| `ashlar run [part]` | build, serve, watch: hot reload preserves state |
| `ashlar fmt` | canonical formatting (comment-preserving, meaning-preserving) |
| `ashlar rename <name> <new>` | rename a space, part, property, or field — atomically, reversibly |
| `ashlar rekind <part.prop> <kind>` | change a merge kind across every layer |
| `ashlar move <part> <space>` | relocate a part, `use` graph rewritten |
| `ashlar radius <name>` | print a rename's complete blast radius, touching nothing |
| `ashlar vendor <source>` | copy a tree in so its spaces resolve (no registry, ever) |

The runtime is a single zero-dependency binary: hand-rolled HTTP/1.1 and
WebSockets on one event loop, live views, session auth (salted iterated
hashing), `stored` persistence, schedules, `spawn`, hot reload, and a
JSON-over-C-ABI foreign function boundary (`foreign/<space>.so`) — the
`dlopen` binding is the only `unsafe` in the codebase.

## Why it's shaped like this

The language is designed for **agents writing code**, so its values are
mechanical, and each has teeth:

- **Guessable.** The whole surface fits in one ≤40,000-byte reference
  (`reference/ashlar.md`), and guessability is *gate-tested*: fresh models
  cold-read program snippets and their misreads are design bugs
  (`suites/t_a3/`, last run 23/24).
- **Diagnostics are corrections.** Stable ids, precise spans, and machine
  edits that always leave the program better: the round-trip metric
  (check → apply fixes → check) converges in a mean of **1.00 rounds**
  over the whole error corpus.
- **Refactors are commands, not text edits.** Blast radius reported first,
  applied atomically or not at all, forward-then-back byte-identical.
- **Fast enough to verify every edit.** A single-file change in a
  1,000-file project re-checks in ~40ms (hard-gated under 100ms).

## Repository layout

| path | contents |
|---|---|
| `reference/` | The complete language reference — the source of truth for every language decision. |
| `docs/` | Vision, requirements, roadmap, diagnostics catalog, and the ADRs (see `docs/README.md`). |
| `AGENTS.md` | The agent-facing working contract — hierarchy, hard rules, sync duties. Load-bearing (T-META enforces it). |
| `examples/` | Twelve complete runnable projects — including `commons`, a full team chat — compiled, format-checked, AND runtime-driven by the suite. |
| `suites/` | Test corpora: the cold-read gate protocol and the loud-failure fixture corpus. |
| `crates/` | The Rust implementation and its 17 test binaries. |

## The hierarchy

Four layers. Each serves the one above it. When two layers conflict, the
higher one wins.

```
VISION          The principles in docs/vision.md. Fixed. If the vision is wrong, stop.
REQUIREMENTS    docs/requirements.md. Revised when it fails to express the vision.
TESTS           The current best encoding of the requirements. Revised freely.
CODE            Whatever makes the tests pass.
```

Code yields to tests. Tests yield to requirements. Requirements yield to
the vision. Nothing overrides the vision.

## Status

Complete against its own definitions: every sentence in the reference has
code and a test behind it, the roadmap ledger (`docs/roadmap.md`) is
empty, and the final increments were adversarially re-reviewed. The suite
is 17 green test binaries in debug and release with zero warnings.
