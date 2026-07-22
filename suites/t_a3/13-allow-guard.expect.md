## Correct reading

`allow` is the authorization guard for the routed part `profile`: the runtime
calls it before `handle`, and a `false` return ends the request with status
403 before `handle` ever runs. Here `allow` returns `true` only when
`req.user` is not `none` — that is, only requests carrying a logged-in
session (`req.user: std.User?`) are let through. No merge kind is stated on
`allow`, so it composes as plain replace.

## Must state

- `allow` is the authorization guard: the runtime calls it before `handle`;
  returning `false` ends the request immediately with status 403, and
  `handle` never runs.
- Here `allow` returns `true` only when `req.user` is not `none` — i.e. only
  requests from a session with a logged-in user (`req.user: std.User?`)
  pass.
- Since no merge kind is stated on `allow`, it composes by plain replace: a
  later layer would fully replace this check, not combine with it.
- Only after `allow` passes does `handle` run, and it receives the same
  `req` value that was checked.
