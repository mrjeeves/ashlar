## Correct reading

`xs[0]` indexes the list's first element; the postfix `!` asserts the
result is present and yields the unwrapped value. Indexing happens first,
then the assertion applies to its result.

## Must state

- `xs[0]` indexes the first element of the list `xs`.
- The postfix `!` is an assertion that the value is present/non-absent,
  yielding the value itself (unwrapping it).
- The order is: index first, then `!` applies to the index's result.
- `xs: [text]` is a list-of-text parameter; `first` is a one-parameter
  function property.
