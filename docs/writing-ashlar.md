# Writing Ashlar code — the traps

For agents authoring examples, fixtures, or tests. Read the reference
in `AGENTS.md` first; it is short on purpose. This page is the
supplement: the mistakes agents actually make when they guess instead of
reading, collected as they happened.

**This file is deliberately not part of `AGENTS.md`.** Anything in
`AGENTS.md` is injected into every in-repo agent's context automatically,
including the readers of the A3 cold-read gate — and a gate that measures
whether Ashlar reads correctly to someone who has never seen it cannot
have the answers sitting in the reader's system prompt. That is not a
hypothetical: it invalidated gate runs 3 and 4
([ADR-0021](decisions/0021-the-a3-readers-were-not-cold.md)). So the
language lives here, behind a path, and `AGENTS.md` links it without an
`@`-import. `t_meta_agents_md_does_not_teach_the_language` keeps it that
way.

## Declarations and locals

- `let` takes no shape annotation; locals are inferred.
- **No shadowing anywhere.** A local or parameter may not reuse any
  visible name — parts, properties, or `std` builtins. Part names like
  `login`, `signup`, `count` collide with builtins or case-fold against
  other names; E002 and E003 will say which.
- Assignment targets must be a `state`/`stored` property of the enclosing
  part, or a `let` local (E025). There is no other mutable thing.

## Functions and blocks

- `=> {` **always** opens a block. To return a map literal, write
  `=> { return { k: v } }`. Writing `=> { k: v }` is a block containing a
  label-looking expression, not a map.
- Function literals live in exactly two places: as a property value, or
  as a call argument (E024). Nowhere else.
- Chain properties (`stack`/`pipe`) restate their kind on **every** layer
  (E004/E005). `pipe` layers must agree in parameter *and* return shape;
  `stack` functions take no parameters, `pipe` functions take exactly one
  (E019).

## Shapes and data

- Map shapes are written `{text: Shape}` — a colon, not a bare element
  type. Computed keys reach data only.
- Optional values need `!= none` before use where a non-optional is
  expected; E006 will offer the edit.
- `append`/`deep` apply to lists and maps, never to a number, bool, or
  function (E028).

## Events and views

- Event handlers receive one `std.Event`. The text of an input is
  `e.data.value`.
- **An instance IS its root element.** Nested `el(Part)` children reuse
  their instance across re-renders — `start` runs once, `stop` on
  removal — so nest freely and lean on the lifecycle instead of hoisting
  state upward.
- Style by `class` name against the root's declared sheet
  (`style = "sheet"` → `assets/sheet.css`). A `style="..."` attribute is
  the wrong tool and will not be maintained by the formatter.

## Settings and deployment facts

- A value the program cannot know when it is written is a `setting`: it
  carries a shape (required — that is the only thing that can check a
  value which does not exist yet) and optionally a default. No default
  means required, and starting without it fails before the first request,
  naming every gap at once.
- `setting` never combines with `state`/`stored` (E030): the value was
  decided at deployment, and a storage word would claim otherwise.
- Deployment supplies values in `settings.json` at the project root, or
  at `ASHLAR_SETTINGS`, keyed by full property name (`site.app.endpoint`).

## When the toolchain disagrees with you

`ashlar check` is right and you are wrong, in this specific sense: the
checker never reports what it cannot prove (requirement A4, "no false
positives"). An error means the fact is established. `ashlar fix` applies
the machine edits; `ashlar fmt` decides formatting, and `fmt --check` is
what the suite asserts, so never hand-format.
