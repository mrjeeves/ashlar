## Correct reading

`counter` renders UI: `view` builds a button element via `el`, wiring the
click to `bump`, which increments the state `n`. `label` is a per-use
field; the button text concatenates the label and the count.

## Must state

- `view` produces a UI element: `el("button", ...)` constructs a button
  with an attribute map and a children list.
- `onclick: bump` wires the button's click to the part's `bump` function,
  and `bump` increments the state property `n`.
- `label: text` is a field with no value (supplied per use), while
  `state n: number = 0` is mutable state starting at 0.
- The child text concatenates `label`, a separator, and `text(n)` — the
  number converted to text.
