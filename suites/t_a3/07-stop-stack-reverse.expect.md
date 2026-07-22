## Correct reading

`stop` is declared `stack reverse` in both layers: all declared stop
functions run, and `reverse` runs them in the opposite of the normal
order — the extending layer's function before the base one's. Both log
and return `none`.

## Must state

- `stack` means both `stop` functions run (neither replaces the other);
  `reverse` runs them in the opposite of the normal/default order.
- Given `reverse`, the `srv.metrics` layer's function runs BEFORE the base
  `srv` one — derived first, base last.
- Both declarations state the same `stack reverse` marking and target the
  same `Server` part.
- Each body logs a message (`log.info(...)`) and returns `none` (no
  value).
