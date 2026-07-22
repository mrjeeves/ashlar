## Correct reading

`pipe` on `handle` means both layers' functions run in sequence, each
receiving the previous one's return value — the logging layer does not
replace the base handler; it processes what the base returns. `route`
associates the part with the URL path.

## Must state

- `pipe` means BOTH `handle` functions run as stages in sequence — the
  second file's function does not override/replace the first's.
- The stages are chained by value: one stage's return value feeds the
  next stage (here `req` passes through both).
- The logging layer logs the request's path (`req.path`) and returns
  `req`.
- `route = "/api/messages"` associates the `messages` part with that URL
  path, and the second file targets the same part via the dotted name.
