## Correct reading

`every = "10m"` makes `sweep` a scheduled task: the runtime calls its `run`
function property on that interval, every 10 minutes here. `every`'s value
is a text duration (digits then a unit), checked at build time. A part with
`every` but no `run` would itself be a compile error.

## Must state

- `every = "10m"` makes `sweep` a scheduled task: the runtime calls its `run`
  function on that interval, here every 10 minutes.
- `every`'s value is a text duration — digits followed by a unit (`ms`, `s`,
  `m`, `h`, or `d`) — checked at build time, not an arbitrary string.
- A part with `every` but no `run` function property is itself a compile
  error; `run` is required once `every` is declared.
- `run` here is a zero-parameter function (not a `stack`/`pipe` composed
  property); each scheduled invocation just calls it directly.
