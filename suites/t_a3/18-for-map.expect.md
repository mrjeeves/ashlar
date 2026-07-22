## Correct reading

`for k, v in counts { ... }` iterates the map `counts`'s entries in key
order, binding `k` (the key, `text`) and `v` (the value, `number`) each
iteration. Each iteration reassigns the state property `lines` to a new list
that appends one formatted entry; this assignment is legal because `build`
is a function declared on `report`, the same part that owns `lines`. Because
iteration is key-ordered, the resulting `lines` is deterministic for a given
`counts` map.

## Must state

- `for k, v in counts { ... }` iterates the map `counts`'s entries in key
  order (sorted by key), binding `k` (the key, `text`) and `v` (the value,
  `number`) each iteration — not insertion order or any other order.
- Each iteration reassigns the state property `lines` to a new list
  (`lines + [...]`), appending one formatted entry; this is legal because
  `build` is a function declared on `report`, the same part that owns
  `lines`.
- Because iteration is key-ordered, the resulting `lines` list is fully
  deterministic for a given `counts` map, regardless of how `counts` was
  constructed.
- Values are immutable in Ashlar: each `lines = lines + [...]` produces an
  entirely new list bound to `lines`, rather than mutating an existing list
  in place.
