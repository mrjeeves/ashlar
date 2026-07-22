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

## Status

### Working today

- The language reference (`reference/ashlar.md`), gated at ≤40,000 bytes.
- Lexer, parser, resolver, and composer.
- JSON Lines diagnostics with machine-applicable fixes where a fix can be
  applied without introducing a new error.
- `ashlar check`, `ashlar fix`, and `ashlar build`, with a manifest fully
  derived from source.
- Test suites: T-A1, T-A2, T-A4, T-B, T-C (as unit tests), T-D,
  T-F (partial), T-META.

### Next

Everything not listed above — including the evaluator and runtime, the
shape checker, the refactor commands, `ashlar fmt`, and the F1 latency
benchmark — is tracked honestly as pending work in
[`docs/roadmap.md`](docs/roadmap.md), each item naming the requirements it
satisfies and the test that will prove it.

## Quickstart

```
cargo build --release
target/release/ashlar check <dir>
```

A minimal program:

```ash
space chat
part app {
  port = 8080
  route = "/"
  handle = (req: std.Request) => "hi"
}
```

## Repository layout

| path | contents |
|---|---|
| `reference/` | The complete language reference. The source of truth for every language decision. |
| `docs/` | Vision, requirements, roadmap, and the ADRs recording how open decisions were resolved. |
| `suites/` | Test suites, each proving specific requirements (see `docs/requirements.md` §9). |
| `crates/` | The Rust implementation: lexer, parser, resolver, composer, diagnostics, manifest, CLI. |

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
