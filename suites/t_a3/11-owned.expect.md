## Correct reading

`owned stored items` declares a persisted list scoped to the current user:
each signed-in user has their own `items`, saved to disk and isolated from
every other user's. `owned` is a per-user modifier on `stored`.

## Must state

- `stored` persists the value on disk, surviving restarts.
- `owned` scopes it to the current user — each user gets a separate copy,
  not one list shared by everyone.
- `items: [text] = []` — a list of text, initially empty.
- `Store` is a part in space `notes.data` declaring only this one property.
