## Correct reading

`counter` is a view part: `view` is a zero-parameter function returning
`std.Element`, built with `el`. `label` is a field, so each use of `counter`
(via `el(counter, { label: ... })`) supplies its own `label` and gets its own
instance of `state n`. `view` renders a `"button"` with an `onclick` attr
naming `bump` and one text child. Clicking the button round-trips the event
over the built-in socket to run `bump` server-side; `bump` reassigns `n`,
and since `view` read `n`, that instance's render re-executes and patches in
place. The browser itself runs no program code.

## Must state

- `counter` is a view part (`view` is a zero-parameter function returning
  `std.Element`); `label` is a field, so each use of `counter` supplies its
  own `label` and gets its own instance of `state n`.
- `view` builds its element tree with `el`: here a `"button"` tag with an
  `onclick` attribute naming the `bump` function property, and one text
  child.
- Clicking the button sends the event over the built-in socket to run `bump`
  server-side (the browser runs no program code); `bump` reassigns `n`, and
  because `view` read `n`, that instance's render re-executes and patches
  the button in place.
- All of this — event round-trip, handler execution, and re-render — happens
  on the server; there is no client-side Ashlar code, only the built-in
  socket protocol.
