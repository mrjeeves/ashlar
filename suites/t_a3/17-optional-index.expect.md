## Correct reading

`m` has shape `{number}`, a map with text keys and number values. Indexing it
with `m["top"]` yields shape `number?`: `none` when the key `"top"` is
absent, the number otherwise. `number?` and `number` are distinct shapes, so
`best`'s inferred return shape is `number?`, not `number`, even though no `?`
is written explicitly anywhere in the body.

## Must state

- `m` has shape `{number}` (a map with text keys and number values); indexing
  it with `m["top"]` yields shape `number?` — `none` when the key `"top"` is
  absent, the value otherwise.
- `number?` and `number` are distinct shapes: a plain `number` never holds
  `none`, so code needing the underlying number must first handle the `none`
  case (with `??`, `!`, or an `if`).
- `best`'s return shape is therefore `number?` (inferred from the body), not
  `number`, even though no `?` is written explicitly.
- This optionality comes specifically from the `[ ]` index operation, not
  from `m`'s own declared shape being optional.
