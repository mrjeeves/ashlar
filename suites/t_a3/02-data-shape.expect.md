## Correct reading

`Message` has only field properties (name and shape, no value), which makes
it a data shape rather than a part with behavior: values of shape `Message`
are written as plain map literals, not constructed by calling anything. `id`
and `body` are required fields with no default, so every literal must supply
both. `read` has a default of `false`, so a literal may omit it, in which
case it is `false`.

## Must state

- `Message` is a data shape (every property is a field: name and shape, no
  value) — values of this shape are plain literals, not instances built via
  a constructor or function call.
- `id` and `body` are required fields (no default): any literal value of
  shape `Message` must supply both.
- `read` has a default of `false`, so a literal may omit the `read` key; when
  omitted, its value is `false`.
- A literal that omits `id` or `body`, or that includes a key `Message` does
  not declare, is a compile error (shapes are checked against the part's
  declared fields).
