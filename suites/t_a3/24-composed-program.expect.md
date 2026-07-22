## Correct reading

Three files. `chat.data` declares `Message` and `Store` (a persistent
map of messages plus an `add` function). `chat.audit` extends the SAME
`Store`, and its `add` — carrying no combining marker — replaces the
base one, adding logging. `chat.api` (which uses `chat.audit`) serves a
route that calls `Store.add` and returns `Store.messages`.

## Must state

- `chat.audit`'s `part chat.data.Store` extends the SAME `Store` declared
  in `chat.data` (dotted name), and its `add` — with no combining marker —
  REPLACES the base `add`, so the logging version is the effective one.
- `add` inserts the message into `messages` keyed by its id
  (`put(messages, m.id, m)` — map, key, value).
- `stored messages: {text: chat.data.Message} = {}` is persistent state:
  a map of Messages with text keys, starting empty.
- The `chat.api` handler calls `chat.data.Store.add(...)` with a new map
  literal and returns `chat.data.Store.messages`; its `use chat.audit`
  line is what gives it that visibility.
