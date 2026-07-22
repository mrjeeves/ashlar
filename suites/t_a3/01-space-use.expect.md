## Correct reading

Two spaces are declared. `chat.data` declares a part `Message` with one field,
`body: text`. `chat.ui` declares `use chat.data`, then a part `Feed` with one
field, `latest: Message?` — an optional value of the `Message` shape. The
`use` line is what makes the bare name `Message` (declared in `chat.data`)
resolvable inside `chat.ui`; without it, `Feed`'s reference to `Message` would
not resolve to anything. `Feed`'s full name is `chat.ui.Feed`.

## Must state

- Two spaces are declared, one per `space` header (`chat.data`, `chat.ui`);
  `Feed`'s full name is `chat.ui.Feed` (space joined to declared name).
- `use chat.data` in `chat.ui` is what brings `chat.data`'s parts — here,
  `Message` — into scope, so `Feed` can reference it by the bare name
  `Message`.
- The bare reference `Message` resolves to `chat.data.Message` specifically;
  there is only one visible definition, so no qualification is required.
- Without the `use` line, the reference to `Message` inside `chat.ui` would
  not resolve at all — visibility comes only from `use`, never from file
  adjacency, order, or any other mechanism.
