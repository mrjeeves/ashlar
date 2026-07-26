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
  view = () => el("button", { class: "count", onclick: bump }, [label + ": " + text(n)])
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

**See the whole language running, in one command.** From a fresh clone — it
builds the release binary itself if you don't have one. Needs **Rust 1.65 or
newer** — there are no crates to fetch. Sixteen of the seventeen examples need
nothing more; `ledger` also needs SQLite's development package, because it links
the real library (`sudo apt install libsqlite3-dev`, or `sqlite-devel` on
Fedora). Without it the other sixteen still run and the launcher says so:

```
./showcase/serve.sh          # macOS / Linux
./showcase/serve.ps1         # Windows (or pwsh anywhere)
```

Either one starts all seventeen examples, each on its own port, then tells you to
open **<http://127.0.0.1:8080>** — the gallery: a sidebar of the other sixteen
with **live frames** you swap by click. Ctrl-C stops every example at once.

The gallery is itself an Ashlar program (`examples/gallery`), and it is the
sharpest demonstration of what a `setting` is for: it renders sixteen addresses
and its source contains none of them. They arrive from
`examples/gallery/settings.json`, which is deployment's file, so a test can
assert the launchers and the gallery agree about every port. Before settings
existed this page could not be written in Ashlar at all — it was hand-written
HTML opened over a local file URL, outside the language it advertised
([ADR-0020](docs/decisions/0020-settings-and-what-b5-actually-forbids.md)).

The frames are the real servers, not screenshots, so what you see is what the
code does. Then run one on its own, or use the toolchain directly:

```
cargo build --release
target/release/ashlar run examples/chat     # one example → http://127.0.0.1:8080
target/release/ashlar check <dir>           # diagnostics as JSON Lines (--human for prose)
```

Every command in the reference's toolchain table exists and is tested:

| command | effect |
|---|---|
| `ashlar check` | compile; diagnostics as corrections (JSONL, `--human` for prose) |
| `ashlar fix [id]` | apply machine-applicable fixes from the last check |
| `ashlar build` | check, then write the manifest |
| `ashlar run [part] [--port n]` | build, serve, watch: hot reload preserves state; `--port` overrides the bound port |
| `ashlar fmt` | canonical formatting (comment-preserving, meaning-preserving) |
| `ashlar rename <name> <new>` | rename a space, part, property, or field — atomically, reversibly |
| `ashlar rekind <part.prop> <kind>` | change a merge kind across every layer |
| `ashlar move <part> <space>` | relocate a part, `use` graph rewritten |
| `ashlar radius <name>` | print a rename's complete blast radius, touching nothing |
| `ashlar vendor <source>` | copy a tree in so its spaces resolve (no registry, ever) |
| `ashlar foreign check` | prove every declared foreign name is reachable before a request finds out |

The runtime is a single zero-dependency binary: hand-rolled HTTP/1.1 and
WebSockets on one event loop, live views, session auth (salted iterated
hashing), `stored` persistence, schedules, `spawn`, hot reload, and a
JSON foreign boundary whose transport is bound in deployment — a native
library, a worker co-process in any language, or an http service (ADR-0017);
the `dlopen` path is the only `unsafe` in the codebase, and the only
platform-specific line in it — `worker` and `http` run wherever Rust runs.

## Why it's shaped like this

The language is designed for **agents writing code**, so its values are
mechanical, and each has teeth:

- **Guessable.** The whole surface fits in one ≤40,000-byte reference
  (`reference/ashlar.md`), and guessability is *gate-tested*: fresh models
  cold-read program snippets and their misreads are design bugs
  (`suites/t_a3/`). The standing result is **24/25 from run 5**, against a bar
  of 20/25. The gate earns its keep: its failures have been real design bugs
  every time — `owned` and `reads`/`writes` both cold-read to the wrong mental
  model and became `peruser` and `watches`/`updates`
  ([ADR-0019](docs/decisions/0019-a3-run3-findings-owned-and-reactive-annotation.md)),
  which run 5 then scored 4/4 clean. How the gate is run, and what counts as a
  cold reader, is `suites/t_a3/PROTOCOL.md`; the per-run results and the
  decisions they produced live beside it and in `docs/decisions/`.
- **Derivable.** Ashlar minimizes semantic freedom so the toolchain can
  compute and explain what names mean, which implementations run, and what
  a change affects ([ADR-0012](docs/decisions/0012-semantic-freedom-and-derivability.md)).
- **Diagnostics are corrections.** Stable ids, precise spans, and machine
  edits that always leave the program better: the round-trip metric
  (check → apply fixes → check) converges in a mean of **1.00 rounds**
  over the whole error corpus.
- **Refactors are commands, not text edits.** Blast radius reported first,
  applied atomically or not at all, and reversing one restores the same
  program. Renaming in place restores every byte; a refactor that must add a
  declaration keeps the line it reported rather than refuse correct work
  ([ADR-0018](docs/decisions/0018-reversibility-is-a-property-not-a-law.md)).
- **Fast enough to verify every edit.** A single-file change in a
  1,000-file project re-checks in ~40ms (hard-gated under 100ms).

## Repository layout

| path | contents |
|---|---|
| `reference/` | The complete language reference — the source of truth for every language decision. |
| `docs/` | Vision, requirements, roadmap, diagnostics catalog, and the ADRs (see `docs/README.md`). |
| `AGENTS.md` | The agent-facing working contract — hierarchy, hard rules, sync duties. Load-bearing (T-META enforces it). |
| `examples/` | Seventeen complete runnable projects — including `commons` (a full team chat), `slate` (a shared pad where two people typing at once are merged line by line, no account and no client-side editor code), `ledger` (a real SQLite datastore over the `foreign` boundary), `locker` (per-user `peruser` storage that isolates each user by construction), `abacus` (a Python worker, no compiler in sight), and `gallery` (the showcase itself, which renders sixteen addresses it learns from a `setting`) — compiled, format-checked, AND runtime-driven by the suite. All wear one dark house style (ADR-0016). |
| `showcase/` | The launchers: `serve.sh` (POSIX) or `serve.ps1` (PowerShell) runs every example on its own port, with the `gallery` on 8080 framing the rest. A test asserts both launchers and the gallery's settings agree on every port. |
| `suites/` | Test corpora and the coverage map: the cold-read gate (protocol, 25 fixtures, per-run results), the 31 loud-failure fixtures, and `coverage.md` — every requirement id to the test that proves it, kept honest by T-META. |
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
code and a test behind it, and the increments were adversarially
re-reviewed. The suite is 17 green test binaries in debug and release with
zero warnings.

One sentence of the reference is currently stricter than the binary, and it is
left that way on purpose. §5 says a literal is checked against the shape its
position expects; a `return` is not yet such a position, so a `pipe` layer that
returns a correct data-shape literal is rejected for returning a map — and, the
other way round, an incomplete literal returned beside a typed one degrades to
`Unknown` and reaches the wire as `null`. Both were found by writing a large
example against hostile input; `examples/slate`'s driving test carries the
reproduction of the second.
The requirement stands and the gap is recorded rather than papered over
([ADR-0025](docs/decisions/0025-a-return-is-a-shape-position.md),
`docs/roadmap.md`).

That same example found two silent defects in `ashlar fmt` — an `else if` in
expression position printed as a block, changing what the program returned and
then deleting the branch; a comment inside a literal migrating onto the next
declaration. Both are fixed with regression tests, and the formatter's
property corpus now covers the examples too
([ADR-0024](docs/decisions/0024-a-formatter-that-loses-code-is-not-a-formatter.md)).

The ledger (`docs/roadmap.md`) carries those two items and one more: a provably
cold A3 read needs a reader outside this repository. A note is carried there
anyway: `25-foreign-reactive` passed run 4 on a 2–1 panel, with the dissent
localizing to `updates` rather than `watches`. It is recorded instead of acted
on, because respelling a keyword off one reader would repeat ADR-0015's mistake
in the opposite direction.
