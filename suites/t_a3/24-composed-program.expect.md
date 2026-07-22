## Correct reading

`chat.data` declares `Message` and `Store`, `Store.add` putting a message
into `Store.messages` (a `stored` map). `chat.audit` uses `chat.data` and
layers `part chat.data.Store` with its own `add`, which logs then does the
same put; since `add`'s kind is unstated (replace) in both layers, the
audit layer's `add` wholly replaces the base's. `chat.api` uses `chat.audit`
(which itself uses `chat.data`), so it transitively sees the fully composed
`Store` — base plus audit layer. Its routed part `messages` calls
`chat.data.Store.add(...)` (the resulting, audit-including definition) and
returns `Store.messages`, rather than assigning `Store`'s state directly,
since only a function on `Store` itself may do that.

## Must state

- `chat.audit`'s `part chat.data.Store { add = ... }` is a layer on the
  existing `Store`, not a new part; because `add`'s kind is unstated
  (replace) in both the base (`chat.data`) and this layer, the layer's
  definition of `add` replaces the base's wholly rather than combining with
  it.
- `chat.api` sees the fully composed `Store` (base definition plus the
  audit layer) because it `use`s `chat.audit`, which itself `use`s
  `chat.data` — `use` is transitive, so `chat.api` gets both spaces' names
  and the composed result reflects both layers.
- Layer order is `chat.data` (base) then `chat.audit` (derived), because
  `chat.audit` uses `chat.data`; this is what makes the audit layer's `add`
  the one that actually runs when `chat.api` calls
  `chat.data.Store.add(...)` — not file order or any other coincidence.
- `chat.api`'s `messages` part calls `chat.data.Store.add(...)` — a function
  property on another part — rather than assigning `Store`'s `messages`
  directly, because only functions declared in a layer of `Store` may
  assign its state properties; other parts must go through such a function.
