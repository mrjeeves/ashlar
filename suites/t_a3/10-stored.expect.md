## Correct reading

`stored` declares persisted state — the word signals durability beyond an
ordinary in-memory variable. `{text: chat.data.Message}` is a map shape:
text keys to `Message` values, initialized empty.

## Must state

- `stored` marks `messages` as PERSISTED/durable data — stronger than a
  plain runtime variable (the word signals storage/survival).
- `{text: chat.data.Message}` is a MAP shape — text keys mapping to
  `Message` values — not a set or a plain collection of Messages.
- `= {}` initializes it to an empty map.
- `Message` is a data shape with two text fields, and `Store` names it by
  its full dotted name `chat.data.Message`.
