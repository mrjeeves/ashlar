## Correct reading

`??` is a coalescing/fallback operator: `name ?? "friend"` yields `name`
when it is present and `"friend"` when it is absent. `name: text?` marks
the parameter optional.

## Must state

- `??` is a fallback/coalescing operator: the left operand when present,
  the right (`"friend"`) when the left is absent.
- `name: text?` declares an optional parameter — the `?` means `name` may
  be absent/none.
- `make` therefore always produces a usable text: the caller's name or
  the fallback `"friend"`.
- `make` is a one-parameter function property on the `greeting` part.
