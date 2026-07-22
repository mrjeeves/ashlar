## Correct reading

`[...base, extra]` builds a new list from base's elements plus `extra` at
the end. `{ ...base, ...patch }` builds a new map from both maps'
entries, with `patch` winning on shared keys.

## Must state

- `[...base, extra]` spreads `base`'s elements into a new list and
  appends `extra`.
- `{ ...base, ...patch }` combines both maps' entries into a new map; on
  a shared key the later spread (`patch`) provides the value.
- These build NEW values; the reading should not claim `base`/`patch` are
  modified in place.
- `extend` and `merge` are function properties with typed list/map
  parameters (`[text]`, `{text: text}`).
