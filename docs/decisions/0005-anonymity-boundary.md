# ADR-0005: Function literals are legal in exactly two positions

Date: 2026-07-22

Status: accepted

## Context

Requirements §11 flags the anonymity boundary as unresolved: anonymous
inline functions are hostile to E2 (after a refactor, no stale reference to
prior state may exist anywhere in the program, checkable by exhaustive
search) because a function with no name cannot be found by that search, cannot
be renamed, and cannot be reported as a location in a blast-radius
computation (E3). At the same time, forcing every single-use callback to
have a declared name would be needless ceremony for the common case of a
one-line handler passed straight into a call — that would cost surface
budget (A5) for no corresponding safety benefit, since a callback that
lives and dies at its call site has no "elsewhere" for a stale reference to
hide in.

The working position recorded in the requirements was: anything reusable
must be named, anything single-use may be inline. What was missing was a
precise, mechanically checkable rule for where the line falls.

## Decision

A function literal is legal in **exactly two positions**:

1. As the value of a property — the property's own name *is* the
   function's name (`bump = () => { n = n + 1 }`).
2. Inside an argument of a call, where it is single-use and moves
   atomically with its call site (`std.sort(xs, (a, b) => a.n < b.n)`).

A function literal may not be bound with `let`, stored in a list, map, or
field, or returned from another function (reference §7). Any function
literal found outside these two positions is a compile error, E024, with a
fix note directing the author to either name it as a property or inline it
directly at the call.

The distinction that falls out of this rule is: a **named** function — a
property whose value is a function — is a first-class value once it has a
name. `Part.save` can be passed around, stored, and referenced anywhere a
value can go, precisely because it has a name the toolchain can find, rename,
and track through a refactor. An *anonymous* function literal never
acquires that status; it is inseparable from the single call it appears in,
so a rename or move of that call moves the whole function with it — there
is no way for it to become "the same anonymous function referenced from two
places," which is the actual hazard E2 is guarding against.

This also settles reusability empirically rather than by asking the author
to judge it: a function becomes reusable exactly when it is written down as
a property, because that is the only way to give it a name, and giving it a
name is the only way to make it referenceable from more than the one call
site it's textually attached to. There's no separate "is this reusable?"
judgment call left over.

## Consequences

- E2 (exhaustive-search absence of stale references after a refactor) is
  computable for functions specifically because every function that could
  be referenced from more than one place already has a name — an
  exhaustive search for references to a renamed property will find every
  use, and there is no anonymous-but-aliased function that could be missed.
- Passing a handler that needs to be reused in two different calls forces
  the author to name it as a property first, which is a small amount of
  friction traded directly for full refactor safety — this is the A5
  trade made explicit: naming costs a line, staying anonymous would cost
  E2 entirely for that value.
- Enforcement is a single, mechanical rule (E024: legal iff property value
  or call argument) rather than a judgment about what "reusable" means,
  which keeps the diagnostic's correction applicable without judgment
  (D1/D2).
