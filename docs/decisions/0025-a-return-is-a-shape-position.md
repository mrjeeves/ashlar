# ADR-0025: A return is a shape position

Date: 2026-07-26

Status: accepted; implemented 2026-07-27 (see Resolution)

## Context

Reference §5 says: *"A literal is checked against the shape the position
expects. For a data-shape part: every field without a default must be
present, every present key must be a declared field, every value must
match the field's shape."*

That is true where the expected shape is handed to the checker — a call
argument, a field default, a property with a declared shape. It is not
true at a `return`, because a function's return shape is inferred from its
returns rather than pushed into them. Writing `quarry` produced both
halves of the consequence.

**A correct program is rejected, with a correction its author has already
followed.** A `pipe` layer over a data shape returning a complete,
correctly-shaped literal:

```
part probe.S {
  chain pipe = (v: probe.V) => {
    return { a: v.a + "!", n: v.n + 1 }
  }
}
```

```
error[E006] pipe layers of `probe.S.chain` must agree in return shape:
  this layer returns `{text: data}`, but the layer in `base.ash` returns `probe.V`.
  fix: Make every layer return `probe.V`.
```

Every field is present and correctly shaped; the literal *is* a `probe.V`.
What the checker means is "I did not know to check it against `probe.V`,
so I inferred a map." The correction as stated cannot be applied — the
author believes they already did it. The move that works is to route the
literal through a call whose parameter names the shape, and nothing in the
reference says so. `guardrails` carries a property for exactly this
purpose (`Gate.keep = (d: Decision) => d`), and `quarry` now carries the
same one (`Store.keep`). An identity function existing in two of the
repo's own examples to give a literal a shape is the language telling on
itself.

**And the reverse: an incomplete literal is accepted, silently.** When one
`return` in a block is a map literal and another is a data shape, the two
shapes do not join, the block's shape degrades to `Unknown`, and `Unknown`
absorbs everything downstream:

```
part S {
  four pipe = (v: V) => {
    if v.n > 5 {
      return { a: "hot" }     // `n` is required and missing
    }
    return v
  }
}
```

This compiles with zero diagnostics, and the served response is
`{"field_a":"hot","field_n":null}` — a required field arriving as `null`
at runtime, from a program the checker called clean.

The `Unknown` degradation is right in itself: requirement A4 and this
repo's rule 7 say the checker never reports what it cannot prove, and a
wrong error would cost more than a missed one. What is wrong is that the
checker *could* prove this. Both shapes are known and they disagree, and
D3 admits exactly two categories — detected at compile time, or documented
in the reference as undetectable with the reason. Division by zero and `!`
on `none` are the documented pair. This is a third category: statically
decidable, undetected, undocumented.

## Decision

**The expected shape belongs at the `return`.** Where a function's return
shape is already fixed by something other than its own body — a `pipe`
property's agreed shape being the load-bearing case — that shape is pushed
into the return expressions and literals are checked against it, exactly
as they are in an argument position. Where two returns in one block carry
known, disagreeing shapes, that is a diagnosis, not a degradation to
`Unknown`.

Both halves fall out of the same change, which is why they are one
decision: with the expected shape present, the correct literal above
compiles and the incomplete one above is an E006 naming the missing field.

**`keep`-style identity properties are then removable**, from
`guardrails` and from `quarry`. They are the workaround, not the idiom,
and the reference should not have to teach them.

## Why the implementation is not in this increment

The change is not the two lines it looks like. A `pipe` layer's agreed
shape is computed today in a pass that runs *after* body inference, over
already-inferred layer shapes; pushing it into the body means establishing
that shape in a pre-pass (`refine_recursive_returns` is the precedent) and
threading an expected shape through block walking. Done carelessly, the
two available shortcuts both damage something the repo values more than
this fix: re-checking a literal after inference double-reports diagnostics
for one expression, and diagnosing "returns disagree" without first
propagating the expected shape would reject the correct program above —
turning a missing error into a false one, which A4 ranks as the worse
failure.

So the direction is decided and the work is scheduled rather than
improvised. The reproductions above are the acceptance test: the first
must compile, the second must not.

## Consequences

- `reference/ashlar.md` §5's sentence is, today, broader than the binary.
  It is left as the requirement it states — the reference describes what
  must be true, and the gap is recorded here and in the roadmap rather
  than by weakening the sentence to match the implementation. This ADR is
  the reason a reader may find §5 stricter than `ashlar check`.
- Until the change lands, a `pipe` layer over a data shape needs a shaping
  call, and `quarry.data.Store.keep` says so in a comment that names this
  ADR's cause rather than pretending the property is a design.
- The corpus gains the second reproduction as a fixture when the work
  lands: a program that compiles clean and serves `null` for a required
  field is a T-A4 case (loud failure) that currently is not loud.

## Resolution (2026-07-27)

Implemented; both halves closed by one change, as predicted. Two things
this decision left open, because neither could be settled without writing
the pass:

**What counts as "fixed by something other than its own body"?** The
property's **declared shape** — written down, so it decides. Otherwise,
**exactly one nominal data shape among the return branches of the
property's layers**: the `(v: V) => v` base layer, whose return comes from
a parameter annotation. Two distinct nominal shapes establish nothing —
that is a real disagreement, already reported. **Absence is the safe
answer**: where nothing is established the checker infers as before, so
the pass can only add precision, never invent it. The established shape
also becomes the property's return shape in the tables, outranking what
`refine_recursive_returns` read off the bodies — without that, a layer
returning a map literal refines the whole chain to `{text: data}` and
every caller of `Gate.review(...).allowed` loses its fields. The first run
did exactly that to `examples/guardrails`.

**How far does the expectation reach?** To **map literals at a `return`**,
and no further. A function whose one branch returns `V` and whose other
returns `none` has return shape `V?` and is correct; pushing `V` onto
every return rejects it — the exact trade A4 forbids. A map literal is the
one form that cannot be wrong about what it *is*. Relatedly, the
established shape seeds the agreement baseline only where two or more
layers exist: with one layer there is nothing to agree with, and a
baseline drawn from that layer's own branches reports it for disagreeing
with itself. The `none` test caught that on the first run.

`Gate.keep` is gone from `guardrails`. The correction points the right way
— seeded by file order it read `Make every layer return `{text: data}``,
deleting the annotation that was right. §5's sentence is true of the
binary again, so the note above about §5 reading stricter than `ashlar
check` no longer applies. One bound: a **list** literal at a `return` is
still inferred, and that narrowness is what protects `return none`.
