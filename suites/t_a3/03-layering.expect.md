## Correct reading

File 2's `part chat.data.Message` names the part file 1 already declared —
the dotted name refers to the existing `Message`, extending it with an
`audit` field rather than declaring an unrelated type. The combined
`Message` has `id`, `body`, and `audit`.

## Must state

- `part chat.data.Message` in file 2 refers to the SAME part declared in
  file 1 — an extension/augmentation of the existing `Message`, not a new
  unrelated type that happens to share the name.
- The extension adds the field `audit: text` with default `"none"`; the
  combined `Message` carries `id`, `body`, and `audit`.
- `use chat.data` is what makes `chat.data.Message` referenceable from
  `chat.audit`.
- `"none"` here is a text value (a string), not a null-like language
  constant.
