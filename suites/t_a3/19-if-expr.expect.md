## Correct reading

`if read { "seen" } else { "new" }` is used here as an expression: it
produces a value, bound to `status` via `let`, rather than merely branching
between statements. `if` is usable as an expression only when both branches
are present and yield one shape — here both yield `text`. The condition
`read` must be `bool`; there is no truthiness in Ashlar.

## Must state

- `if read { "seen" } else { "new" }` is an expression here, not just a
  statement: it produces a value that is bound to `status` via `let`.
- `if` is only usable as an expression when both branches are present and
  both yield the same shape — here both branches yield `text`.
- The condition `read` must have shape `bool`; there is no truthiness in
  Ashlar, so any other shape as a condition is a compile error.
- `status` is a `let`-bound local: single-assignment, its shape (`text`)
  inferred from the `if` expression.
