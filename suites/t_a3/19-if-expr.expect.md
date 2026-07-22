## Correct reading

The `if`/`else` is used as an expression: it produces one of the two
texts, and `let` binds that value to `status`, which the function
returns.

## Must state

- The `if read { "seen" } else { "new" }` construct is used as an
  EXPRESSION producing a value — not merely a branch statement.
- Both branches yield a text; whichever branch runs, its text becomes the
  bound value.
- `let status = ...` introduces a local binding holding that result, and
  `return status` returns it.
- `read: bool` is the condition parameter of the `describe` function.
