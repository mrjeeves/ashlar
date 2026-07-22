## Correct reading

`[...base, extra]` builds a new list by copying `base`'s entries in order,
then appending `extra`. `{ ...base, ...patch }` builds a new map by copying
`base`'s entries, then `patch`'s; when both have the same key, the later
spread's value wins, so `patch`'s value overrides `base`'s for a shared key.
Both functions are ordinary value properties; spread is a property of the
literal expression itself, evaluated fresh each call, and it does not mutate
`base` or `patch`.

## Must state

- `[...base, extra]` builds a new list by copying every entry of `base` in
  order, then appending `extra` — spread copies entries, it does not merge
  or dedupe the way `append`/`deep` layering does.
- `{ ...base, ...patch }` builds a new map by copying `base`'s entries then
  `patch`'s; when both have the same key, the later spread's value wins
  (`patch`'s value overrides `base`'s for a shared key).
- Both `extend` and `merge` are ordinary value-property functions (no
  storage word, no merge kind); spread is a property of the literal
  expression itself, evaluated fresh on every call — unrelated to part
  layering.
- Spread only copies entries into a new literal; it does not mutate `base`
  or `patch` themselves (values are immutable throughout Ashlar).
