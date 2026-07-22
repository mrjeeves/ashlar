## Correct reading

`chat.data` declares the part `Message` with fields `id` and `body`.
`chat.audit` uses `chat.data` and then declares `part chat.data.Message`, a
dotted name matching that existing part exactly. This does not create a
second, separate part; it adds a **layer** onto the one `Message` part,
contributing an additional field `audit` with default `"none"`. Wherever
`Message` is referenced in the composed program, it has `id`, `body`, and
`audit`. Because `chat.audit` uses `chat.data`, the audit layer sits after
(on top of) the base layer.

## Must state

- `part chat.data.Message` in `chat.audit` is a layer on the existing part
  `chat.data.Message`, not a new, separate part — the dotted name must match
  a part already visible through `use`.
- The composed `Message`, wherever referenced, has all of `chat.data`'s
  fields (`id`, `body`) plus `chat.audit`'s added field `audit` (default
  `"none"`).
- Layer order follows the use graph: because `chat.audit` uses `chat.data`,
  the audit layer sits after the base layer — this is computed from the
  declarations, never from file location.
- A dotted part name that matched no visible part would be a compile error
  (naming the nearest match), so this only works because `chat.data.Message`
  genuinely exists and is visible via the `use` line.
