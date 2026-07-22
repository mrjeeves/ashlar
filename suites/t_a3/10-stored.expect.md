## Correct reading

`messages` is a `stored` property of `Store`: shape `{chat.data.Message}` (a
map, keyed by text, of `Message` values), initialized to the empty map `{}`.
Unlike `state`, a `stored` property is persisted by the runtime's embedded
store, keyed by the property's full name, and survives process restarts. On
startup its persisted value is validated against the current shape, and a
mismatch is a startup error. Only a function declared on `Store` may assign
`messages`; other parts may read it, or call such a function.

## Must state

- `messages` is a `stored` property: the runtime's embedded store persists
  it, keyed by the property's full name, so its value survives process
  restarts — unlike `state`, which lives only for the process.
- Its shape is a map of `chat.data.Message` values keyed by text,
  initialized to the empty map `{}`.
- On startup the runtime validates the persisted value against this shape; a
  mismatch is reported as a startup error rather than silently accepted.
- Only functions declared on `Store` itself (a layer of the same part) may
  assign `messages`; other parts may only read it by name or call a function
  on `Store` that assigns it.
