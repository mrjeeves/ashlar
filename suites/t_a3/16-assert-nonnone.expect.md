## Correct reading

`xs[0]` has shape `text?` (list indexing yields an optional). The postfix `!`
asserts the value is not `none` and yields the plain `text`; access binds
tighter than `!`, so the index happens first, then the assertion applies to
its result. If `xs` is empty, `xs[0]` is `none` at runtime and `!` raises a
runtime fault at that location, failing the surrounding request or task —
there is no way to catch it in-language.

## Must state

- `!` is postfix "assert non-none": `xs[0]!` asserts that `xs[0]` (shape
  `text?`, since list indexing yields an optional) is not `none`, and yields
  the plain `text` value.
- Precedence: access (`.`, `[ ]`, `( )`) binds tighter than `!`, so `xs[0]!`
  first computes the index `xs[0]`, then applies the assertion to that
  result.
- If `xs[0]` is `none` at runtime (e.g. `xs` is empty), `!` raises a runtime
  fault at that location; the current request or task fails, and there is no
  way to catch it in-language.
- This is one of exactly two runtime faults the language defines (division
  by zero is the other); both are undetectable at build time since they
  depend on runtime values.
