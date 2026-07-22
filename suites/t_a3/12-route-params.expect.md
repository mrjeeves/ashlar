## Correct reading

`{id}` in the route text is a path-parameter capture. The handler reads
the captured value by indexing the request's params map with the same
name: `req.params["id"]`.

## Must state

- `route = "/api/messages/{id}"` associates the part with a URL path in
  which `{id}` is a parameter/capture segment, not literal text.
- `req.params["id"]` retrieves the captured path value by indexing a
  params collection on the request with the capture's name.
- `handle` receives the incoming request as `req` of shape `std.Request`.
- The handler's body is the expression `req.params["id"]` — its value is
  what the handler produces.
