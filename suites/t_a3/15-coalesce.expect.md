## Correct reading

`??` is none-coalescing: `name ?? "friend"` evaluates to `name` when `name`
is not `none`, and to `"friend"` only when `name` is `none`. `name` has shape
`text?`; since `??` removes the `none` possibility, the whole expression has
shape plain `text`. This tests specifically for `none` — Ashlar has no
truthiness, so an empty string would still pass through unchanged rather
than triggering the fallback.

## Must state

- `??` is none-coalescing: `name ?? "friend"` evaluates to `name` when it is
  not `none`, and to `"friend"` only when `name` is `none`.
- `name` has shape `text?` (optional text); because `??` removes the `none`
  possibility, `name ?? "friend"` itself has shape plain `text`.
- `??` specifically tests for `none` — not any other falsy-like value (there
  is no truthiness in Ashlar) — so an empty string in `name` would still
  just pass `name` through unchanged.
- `make` is a plain value property (a function bound with `=`, no storage
  word and no merge kind), so it is a fixed, non-reactive, non-layered
  definition.
