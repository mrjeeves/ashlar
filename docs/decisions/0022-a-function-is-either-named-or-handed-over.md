# ADR-0022 — A function is either named or handed over

**Status:** accepted and applied, 2026-07-25.
**Clarifies:** `E024`, reference §7's "where functions may appear".
**Found by:** writing `examples/gallery`, which could not compile.

## The contradiction

Two sentences of the reference disagreed, and had for as long as both existed.

§7 said a function literal "cannot be bound with `let`, stored in a list, map,
or field, or returned from another function." §9.4 said "An attr value is text,
or the name of a function property, or an inline function taking zero parameters
or one (`(e: std.Event) => ...)`."

An attr map *is* a map. So an inline handler was simultaneously the documented
way to write a click handler and a thing §7 forbade. The resolver implemented
§7's clause — map-literal values were walked with function literals disallowed
— and the renderer implemented §9.4: `render_attrs` has always matched
`V::Fn(_)`, registered the closure under a generated handler id, and dispatched
browser events to it. The evaluator has always produced closures.

So the capability was fully built and unreachable. Nothing in the corpus caught
it because nothing in the corpus had tried: every example that needed a handler
over a list item introduced a child view part with fields, which is the
`commons` idiom and works fine. The gallery is the first program where that is
absurd — a sidebar button whose only job is to select the item it was rendered
from does not need its own part.

## The decision

§9.4 wins, and §7 gets the rule it was reaching for.

The distinction that matters is not which bracket the function sits inside. It
is whether the function **outlives the expression that wrote it**:

- **Named** — the value of a property. The toolchain can rename it, find its
  references, and report it in a blast radius.
- **Handed over** — inside an argument of a call, used for that call and not
  retained under any name. `map(xs, (x: Site) => …)` is this. So is
  `el("button", { onclick: (e: std.Event) => pick(s) }, …)`: the closure goes to
  `el`, which registers it for exactly this render, and no name refers to it.

Everything else is *stored*, and stays an error: bound with `let`, put in a
property's own list or map, kept in a field, or returned from a function. Those
are the cases where a function acquires a lifetime the toolchain cannot see, and
they are what E024 exists to prevent.

Mechanically this is one enum in `resolve.rs`. `FnLit::Here` is a property
value — legal exactly there. `FnLit::Nested` is a call argument — legal there
and inside any list or map literal written at that position, because such a
literal is still lexically at the call site. Everything else is `FnLit::No`.
A property value does *not* propagate: `state saved = { go: () => 1 }` is
still E024, which is the case that keeps the rule meaningful.

`t_b_a_handler_may_be_inline_in_attrs_but_never_stored` pins both directions —
an inline attrs handler closing over a `map` parameter checks clean, and the
four storing forms produce exactly four E024s.

## Why not the other repair

Deleting §9.4's inline-function clause would also have made the reference
consistent, and it was the smaller edit. It was wrong for three reasons:

1. **It would document a lie in the opposite direction.** The renderer would
   still accept a function in attrs; the resolver would still refuse to let one
   be written. Dead reachable code is worse than a documentation bug.
2. **It costs real expressiveness.** Without it, every interactive list row
   needs a part whose fields exist only to carry the row back to a handler. That
   is ceremony an agent has to invent, which is the opposite of the point.
3. **The rule it was defending was never about brackets.** "Stored in a map"
   was shorthand for "retained without a name," and the shorthand happened to
   catch a case that is not retained at all.

## What this does not open

No new syntax, no new diagnostic id, no change to what `el` accepts at runtime.
The `let`, field, and return prohibitions are untouched, and a function still
cannot be smuggled through a property's own literal. E2 — no stale reference
after a refactor — is unaffected: an unnamed single-use closure has no reference
to go stale, which is the same reason a call argument was always allowed.

The corpus caught this only because a program was written that needed it. That
is the argument for `examples/` being corpus rather than decoration: the gallery
found a two-sentence contradiction that had survived every reference pass.
