## Correct reading

`state n` declares runtime-mutable data initialized to 0 — distinct from
a plain immutable binding. `bump` is a function on the same part that
increments `n` by reassigning it.

## Must state

- `state n: number = 0` declares runtime-MUTABLE data with initial value
  0 — the `state` word distinguishes it from an ordinary fixed binding.
- `bump` is a function property whose body `n = n + 1` increments `n` by
  reassignment.
- `bump` can assign `n` because both belong to the same part (`Counter`);
  the assignment references the property directly by name.
- `Counter` is a part in space `counter` holding exactly this state and
  this function.
