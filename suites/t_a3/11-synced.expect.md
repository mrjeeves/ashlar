## Correct reading

`online` is a `synced` property: like `state`, it is runtime-mutable and
lives for the process, but every change to it is additionally pushed to all
connected clients whose views read it, automatically, over the built-in
socket — there is no explicit channel or publish call needed. Its initial
value is `0`. As with any state-class property, only a function declared on
`Room` may assign it.

## Must state

- `online` is a `synced` property: like `state` (process-lifetime,
  reactively read), but every change is additionally pushed to all connected
  clients whose views read it.
- Its initial value is `0`; being a state-class property (not a value
  property), it is runtime-mutable, not a fixed build-time constant.
- Propagation to clients happens automatically over the built-in socket
  mechanism — no explicit channel or `publish` call is needed for `synced`.
- Only functions declared in a layer of `Room` may assign `online`; other
  code may only read it, or call such a function.
