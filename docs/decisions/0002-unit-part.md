# ADR-0002: The composable unit is named `part`

Date: 2026-07-22

Status: accepted

## Context

Requirement C1 says there is exactly one composable unit, and UI elements,
routes, services, state stores, and data shapes are all the same kind of
thing composed by the same mechanism. Requirements §11 flags the name of
this unit as an open decision that "must survive T-A3" (guessability) and
lists candidates that all carry a collision with an existing, narrower
concept from some other ecosystem: `component` (UI prior), `type`
(type-system prior), `entity` (ECS/DDD prior), `model` (ML/MVC prior).

Each of those priors is a problem for A4: a reader who has used React would
guess `component` means something UI-specific and be wrong the moment it's
used for a route or a data shape; a reader who has used a typed language
would guess `type` means a type and be wrong the moment it has `state`
properties and a `start` lifecycle function. The candidates were rejected
one by one against that failure mode, not on taste.

## Decision

The composable unit's keyword is **`part`** — `part Name { properties }`,
as in the reference §3.

`part` carries one prior, C#'s `partial` (a class split across several
`partial` declarations that merge into one type at compile time). That
prior is *aligned*, not misleading: Ashlar's part is also several
declarations — one per layer, one per space that adds to it — merging into
one thing at build time. Alignment between what a reader already believes
and what the construct does is rare enough among the candidates that it
was decisive. `partial`-as-prior is also rare enough as a memory in most
readers (compared to `component` or `type`) that it costs little should it
not fire, and pays for itself when it does.

Rejected candidates, with the specific collision each fails on:

- **`component`** — UI-framework prior (React, Vue, Web Components). A
  reader would expect it to mean a UI element specifically; Ashlar uses the
  same keyword for routes, services, and data shapes.
- **`type`** — type-system prior. A reader would expect a type declaration
  with no runtime state or lifecycle; Ashlar's part has `state`, `route`,
  `start`/`stop`, none of which belong to a "type" in any prior sense.
- **`entity`** — ECS (entity-component-system) and DDD ("entity" as an
  object with identity) priors. Both carry specific baggage — components
  attached to entities, identity vs. value semantics — that Ashlar's part
  does not match.
- **`model`** — ML ("a model") and MVC ("the model layer") priors, both
  extremely active in current usage. A reader would guess wrong in either
  direction: not a trained artifact, not specifically the data layer of an
  MVC split.
- **`block`** — reads as a body of code (a `{ }` block), not a named,
  composable, singleton declaration.
- **`thread`** — concurrency prior, and Ashlar's part has nothing to do
  with threads.
- **`unit`** — collides with ML's unit value (the "nothing" return type)
  and with "unit" as in "unit test." Both actively wrong in context.
- **`element`** — DOM prior. Fatal specifically because Ashlar is a
  language for building interfaces: `element` would be read as "DOM
  element" by every reader who has touched a browser, which is most of
  them, and a part is not only ever a UI element.

`part` is also short, pluralizes regularly (`parts`), and reads correctly
cold in the canonical example — `part chat.ui.Message { }` — without
requiring the reader to already know Ashlar's model of composition.

## Consequences

- The requirements document itself needed correction because of this
  decision: §0 originally claimed "the language's own name is the
  most-repeated name in the system." That's false once `part` exists — a
  part keyword appears once per declaration, in every file, while the
  language's own name (`Ashlar`/`ashlar`/`.ash`) appears only in the CLI,
  the manifest header, the file extension, and diagnostics. §0 has been
  revised accordingly (see docs/requirements.md, "Revisions"), and the
  T-A3 guessability budget is understood to be spent gating `part` and the
  rest of the surface syntax, not the language's own name.
- Because `part` is the single unit for UI elements, routes, services,
  state stores, and data shapes alike (C1), every future builtin capability
  (view, route, files, every/run, foreign) is expressed as a property on a
  `part` rather than as a separate declaration form — there is no second
  top-level noun to design, name, or keep consistent with this one.
- The rejected candidates are burned for this purpose: none of
  `component`, `type`, `entity`, `model`, `block`, `thread`, `unit`,
  `element` should be reconsidered for the unit keyword without a new
  T-A3 corpus result overturning this one.
