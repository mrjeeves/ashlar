## Correct reading

`Config.tags` is declared with merge kind `append` in the base space
`config`, holding `["core"]`. `config.ext` uses `config` and layers `Config`
with its own `tags append = ["extra"]`, restating the same kind. Because the
kind is `append`, the two layers' lists concatenate rather than the later one
replacing the earlier one: the composed `tags` is `["core", "extra"]`, base
layer's items first, then the derived layer's.

## Must state

- `append` is the merge kind for `tags`: when multiple layers declare it,
  their lists concatenate into one list — the later layer does not replace
  the earlier one.
- The composed value of `tags` is `["core", "extra"]`: the base layer's items
  first, then the derived layer's, in use/composition order.
- Every layer that touches `tags` must restate `append` (as `config.ext`'s
  layer does); omitting the kind, or stating a different one, is a compile
  error.
- This merge is computed entirely at build time from the layers' literal
  values — it does not depend on any runtime state.
