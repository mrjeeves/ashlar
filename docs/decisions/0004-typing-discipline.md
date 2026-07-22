# ADR-0004: Static typing, structural for data, nominal for parts

Date: 2026-07-22

Status: accepted

## Context

Requirements §11 leaves typing discipline open, constrained by two
requirements that pull against each other: A1 (the whole reference must fit
in 40,000 bytes) and D1/D2 (every type error must be a location, a one-
sentence cause, and a correction that is safe to apply mechanically). A
type system rich enough to need a page of inference rules is unaffordable
under A1 regardless of its expressive benefit — the requirements explicitly
say a construct that costs 3,000 characters of reference and doesn't return
5% of the language's value gets removed (A5), and a type system is exactly
the kind of feature that can quietly cost far more than it looks like it
costs. D3 additionally requires that every condition the runtime *could*
detect be moved to compile time or explicitly documented as undetectable —
dynamic typing pushes shape errors to runtime, which fails D3 outright.

## Decision

Ashlar is **statically typed**, with two different disciplines for two
different kinds of thing:

- **Structural** for data. A literal is checked against the shape the
  position expects: for a data-shape part, every field without a default
  must be present, every present key must be a declared field, and every
  value must match that field's declared shape (reference §5). There is no
  data "type" separate from its shape — the shape *is* the check.
- **Nominal** for parts. A part's name is itself a shape: referencing a
  part as a shape means "the composed singleton of that part," and two
  parts with structurally identical fields are still different shapes,
  because they are different names.

Function parameters must declare shapes; return shapes and `let` locals are
inferred from their expressions (reference §5, §7) — this keeps the
authoring surface small (no return-type annotations to write, no local
declarations) while keeping every boundary a human or another part depends
on (a call's parameters) explicit and checked.

Optionals are `shape?`, read with `??` (else-branch on `none`) or asserted
with postfix `!`, which traps at runtime on `none` rather than silently
producing a wrong value. `!` was chosen to be the *louder* of the two
competing existing conventions — Swift's trapping `!` versus TypeScript's
non-trapping assertion `!` — specifically because A4 says that where two
existing priors disagree, the one that fails loudly when guessed wrong is
the correct default. A reader who guesses `!` means "trust me, don't
check" (the TS reading) and is wrong gets a runtime fault at the exact
call site, not a `none` silently flowing further into the program.

There is **no `any`**. An unchecked hole in an otherwise checked type
system is exactly the kind of silent-wrongness surface A4 is written to
eliminate — a value typed `any` can be guessed about incorrectly with no
compile-time signal at all.

There is **no truthiness**. Conditions must be `bool`; using any other
shape as a condition is a compile error (reference §6). This removes an
entire class of "is empty-string/zero/empty-list falsy" guesses that differ
across every language a reader might be importing intuition from.

Maps are **text-keyed** (`{shape}`), matching the one universal prior every
likely reader already has: JSON object keys are strings. `data` is the
shape of the JSON universe specifically — text, number, bool, `none`, list
of `data`, map of `data` — and is reserved for payload boundaries (decoded
request bodies, foreign-function crossings) rather than being available as
a general escape hatch from the shape system.

The full shape checker beyond what's described above is the next
implementation increment; error id **E006** is reserved for shape-mismatch
diagnostics ahead of that work landing (docs/diagnostics.md).

## Consequences

- A1's budget is preserved: the entire typing discipline is describable in
  reference §5 without an inference algorithm, because only two rules exist
  (structural-for-data, nominal-for-parts) and only one direction is
  inferred (returns and locals, never parameters).
- D1/D2 are satisfiable for type errors specifically because the check is
  structural at the leaves: "expected field `x: number`, got `text`" is a
  fact with an obvious correcting edit, not a judgment call the way a
  general unification failure message often is.
- Foreclosing `any` and truthiness means every future builtin or shape
  addition must fit the existing structural/nominal split — there is no
  pressure-release valve to reach for if a future feature seems to need
  one; that pressure should be read as a signal the feature needs
  rethinking, not as license to add either back in.
- The shape checker's absence today is tracked as a roadmap item
  (docs/roadmap.md) rather than as a silently-missing feature — E006 being
  reserved rather than assigned to something else is what keeps that debt
  visible in the diagnostic catalog itself.
