## Correct reading

`allow` is an access predicate for the routed part: the request proceeds
only when it returns true, which here requires `req.user` to be present
(not `none`). `handle` then returns the user.

## Must state

- `allow` is a gate/guard predicate: the request is admitted only when it
  returns true, and rejected otherwise.
- `req.user != none` tests presence — only requests carrying a user (a
  logged-in/authenticated session) pass the guard.
- `handle`'s body returns `req.user`.
- `allow` and `handle` both take the request as a `std.Request` parameter
  on the same routed part.
