## Correct reading

`Server` has `port`, so it is the server root: `ashlar run` calls its `start`
property once at launch. `start` is declared `stack` in the base space `srv`
and layered again, still `stack`, in `srv.metrics` (which uses `srv`). Every
layer's function runs in composition order — base layer first, then the
derived `srv.metrics` layer — and each returned map merges one level onto
`Server`'s state: `ready: true` from the base layer and `count: 1` from the
derived layer both end up applied.

## Must state

- `Server` is a server root (it has `port`); on `ashlar run` the runtime
  calls its `start` property once, at launch.
- `start` is a `stack` property: every layer's function runs in composition
  order — the base layer (`srv.Server`'s own definition) before the derived
  layer added in `srv.metrics` (which uses `srv`).
- Each layer's returned map merges one level onto the part's state
  properties: `ready: true` from the base layer and `count: 1` from the
  derived layer both end up applied.
- Layers run base-to-derived by default (the opposite of `reverse`), and the
  call as a whole returns the composed part, not either function's return
  value directly.
