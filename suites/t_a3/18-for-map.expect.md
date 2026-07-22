## Correct reading

`for k, v in counts` iterates the map's entries with key and value bound
each round. Each round rebuilds `lines` by concatenating a formatted
"key: value" text; `lines` is reset to empty first.

## Must state

- `for k, v in counts { ... }` iterates the map's entries, binding the
  key (`k`) and value (`v`) each iteration.
- Each iteration reassigns `lines` to `lines + [k + ": " + text(v)]` —
  list concatenation appending one formatted text entry, with `text(v)`
  converting the number.
- `lines = []` first clears the state list; `build` may assign `lines`
  since both belong to the same part.
- The reading must not actively assert a specific iteration order as a
  fact the snippet establishes (saying the order is unspecified by the
  snippet, or naming an order as a guess, passes; insisting the snippet
  guarantees e.g. insertion order fails).
