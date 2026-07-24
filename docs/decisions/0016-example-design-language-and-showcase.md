# ADR-0016: A shared design language for the examples, and a live showcase

Date: 2026-07-24

Status: accepted

## Context

The examples are corpus and showcase both (`t_examples`, AGENTS.md). Every
one compiled clean and was driven at runtime, but only two — `commons` and
`ledger` — declared a stylesheet; the other twelve rendered with browser
defaults. For a language whose pitch is "servers and interfaces," a wall of
Times New Roman on white undersells the interface half. Four examples
(`diary`, `press`, `guardrails`, `locker`) had no `/` view at all — they
were API/auth demos you could only reach with `curl`, so there was nothing
to *look at*.

Two things were wanted: give every example an elegant, restrained skin, and
add a top-level way to flip through them all quickly.

Nothing here changes the language. ADR-0010 already settled *how* appearance
attaches — a root names a sheet, views carry `class` names, the names are
the joint. This decision is about applying that consistently and about the
review surface around it.

## Decision

**One house style, re-declared per example.** A small dark palette (the
`commons` tokens: `--bg #0f1115`, `--panel`, `--line`, `--accent #5b8cff`,
system font, soft shadows, 10–16px radii) and a shared component vocabulary
(`.stage`/`.card`, `.field`, `.primary`/`.ghost`, `.kicker`). Each example's
`assets/<name>.css` stands alone — no shared base file to import, because
the runtime serves exactly one sheet per project and zero-dependency means
zero shared build inputs. Cohesion comes from repeating the same tokens, not
from linking a common file. Restraint is the rule: no example out-dresses
what it teaches.

**The four API-only examples get a small `/` view** that demonstrates their
idea in the browser — `press` a live preview of the composed pipe,
`guardrails` a live policy verdict, `diary` and `locker` a login gate and a
per-user page. Their tested API routes are untouched; the views are
additive and are themselves driven by `t_examples`.

**The showcase is a live launcher, not a static gallery.** `showcase/` holds
one self-contained `index.html` (a sidebar of all fourteen, an iframe that
swaps on click or arrow-key) and `serve.sh`, which runs every example at
once, each on its own port. The frames show the *running* apps. A baked
snapshot of each example's HTML was the obvious alternative and was
rejected: it would be a second copy of every UI with no test keeping it
honest, exactly the kind of drift the "examples are driven at runtime" rule
exists to prevent. The app is the only source of truth.

**`ashlar run --port N`** makes the launcher possible without editing a line
of any example. The source keeps `port = 8080`; the flag overrides where it
serves. That is squarely B5: a location (here, a port) is a deployment fact
bound at run time, never written in source. It is useful well beyond the
showcase — any time two projects must run side by side.

A viewport `<meta>` now ships in every served page's head, so the examples
are legible on a phone and `commons`'s existing responsive rules finally
apply.

## Consequences

- All fourteen examples share a look; the showcase reads as one family.
- `t_examples` grew: the todo list renders real rows (its one assertion
  moved from the joined string to the row), and the four new `/` views each
  gained a runtime assertion — the composed pipe/policy rendering in the
  view, the owned board rendering a user's own notes. Nothing was weakened.
- `--port` is a new toolchain flag: reference §9.1 and the §11 table carry
  it, `cli::parse` parses it, and unit tests pin both the parse and a bad
  value.
- The showcase is dev convenience, not corpus: it lives outside `examples/`
  and is not a test target. `serve.sh` builds `ledger`'s SQLite shim first
  and skips it loudly where the toolchain is absent, matching the driving
  test.
