## Correct reading

`stack` on `start` means both layers' functions run when start happens —
the metrics layer's function does not replace the base one. Each returned
map updates the matching `state` properties: `ready` becomes true and
`count` becomes 1.

## Must state

- `stack` means BOTH declared `start` functions run — the second file's
  function joins the first's rather than replacing it.
- Each function's returned map (`{ ready: true }`, `{ count: 1 }`) updates
  the part's matching `state` properties, so both effects apply.
- `state ready: bool = false` and `state count: number = 0` declare
  mutable state with initial values.
- `srv.metrics`'s `part srv.Server` targets the same `Server` part
  declared in `srv` (dotted name + `use srv`).
