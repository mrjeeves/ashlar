## Correct reading

Two spaces, one per header. `chat.data` declares part `Message` with one
text field. `chat.ui` declares `use chat.data`, then part `Feed` whose
`latest` field is an optional `Message`. The `use` line is what lets
`chat.ui` reference `Message` by name.

## Must state

- Two spaces are declared, one per `space` header (`chat.data`, `chat.ui`),
  and each part belongs to the space whose file declares it.
- `use chat.data` is an import-like declaration: it is what makes
  `chat.data`'s declarations (here `Message`) referenceable from `chat.ui`.
- `latest: Message?` gives `Feed` a field whose value is a `Message` or
  absent — the `?` marks optionality of the field's shape.
- `Message` is a named data structure with one typed field `body: text`;
  `part { ... }` declares such a structure.
