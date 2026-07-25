## Correct reading

Two capabilities implemented outside the language, both naming the same
reactive collection. `all` is declared `reads Row`, which makes calling it a
dependency: a view that calls `all` re-renders when the collection changes.
`save` is declared `writes Row`, which marks the collection changed, so every
view that read it re-renders — across every connected client. The collection
is the `Row` data shape it names.

## Must state

- `foreign` marks `save` and `all` as implemented OUTSIDE this language;
  only their signatures appear here.
- `reads Row` / `writes Row` join the call to reactivity — a read is a
  dependency, a write invalidates it. Stating them as merely documentary, or
  as a return/parameter shape, is a MISREAD.
- `Row` names the collection, and is the data shape declared above it.
- A write through `save` causes views that called `all` to update, with no
  explicit subscription or refresh in this file.

## Watch for

This fixture exists because `reads` and `writes` are CONTEXTUAL: they are
ordinary names everywhere else in the language. A reader who takes
`writes Row` for a property, a parameter, or the return shape has found a
design bug, not a documentation gap.
