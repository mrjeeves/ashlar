## Correct reading

`m` is a map with text keys and number values. `m["top"]` looks up the
key `"top"`, whose presence is not guaranteed — a careful reading keeps
the absent-key case open rather than assuming the lookup always yields a
number.

## Must state

- `m: {text: number}` is a MAP shape — text keys to number values — not a
  set or a plain number collection.
- `m["top"]` looks up the key `"top"` in that map.
- The key `"top"` may be absent at runtime; the reading must not
  positively assert the lookup is guaranteed to produce a number (any
  acknowledgment of the absent case, or silence about guarantees,
  passes; an active claim of guaranteed presence fails).
- `best` is a one-parameter function property whose body is this lookup.
