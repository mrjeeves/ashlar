## Correct reading

`route = "/api/messages/{id}"` makes `message` a routed part; `{id}` is a
capture segment, so whatever text matches that position in an incoming
path is bound into `req.params` under the key `"id"`. `handle` receives the
request as `req: std.Request` and reads the captured value via the index
`req.params["id"]`, which — because indexing a map yields an optional — has
shape `text?`, not plain `text`.

## Must state

- `route = "/api/messages/{id}"` makes `message` a routed part; the `{id}`
  segment is a capture that binds whatever matches that path segment into
  `req.params` under the key `"id"`.
- `handle` is called with the incoming request as `req: std.Request`, and
  reads the captured value via the index `req.params["id"]`.
- `req.params` has shape `{text}` (a map with text keys and text values), so
  indexing it yields `text?` — `none` if the key were ever absent — not a
  plain `text`.
- The same routed part and handler serve both HTTP and the built-in
  WebSocket transport identically; the handler code does not know or care
  which transport is in use.
