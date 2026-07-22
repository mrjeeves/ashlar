## Correct reading

`stop` is declared `stack reverse` in both the base space `srv` and the
derived space `srv.metrics` (which uses `srv`). On shutdown the runtime calls
`stop`, and `reverse` runs every layer's function derived-to-base — the
opposite of the default order — so `srv.metrics`'s function (the derived
layer) runs before `srv.Server`'s own (the base layer). Both functions log a
message and return `none`, so nothing merges onto state.

## Must state

- `stop` is declared `stack reverse`: on shutdown, the runtime calls it,
  running every layer's function in derived-to-base order — the opposite of
  the default (base-to-derived).
- Concretely, `srv.metrics`'s layer (the derived one, since `srv.metrics`
  uses `srv`) runs before the base `srv.Server` layer's own function.
- `reverse` is fixed together with the kind (`stack`) as part of the
  property's identity: every layer touching `stop` must restate both `stack`
  and `reverse` identically, as both layers here do.
- Each layer returns `none`, so nothing merges onto state here; `stop` is
  called once, at shutdown, before stored state is flushed.
