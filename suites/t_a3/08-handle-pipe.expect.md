## Correct reading

`handle` is declared `pipe` on `messages` in `chat.api`, and layered again,
still `pipe`, in `chat.api.logging` (which uses `chat.api`). Calling `handle`
runs every layer's function in composition order, and each layer after the
first receives the previous layer's return value as its argument. The base
layer returns `req` unchanged; the derived logging layer receives that same
`req`, logs its path, and returns it again. The call's overall result is the
last layer's return value.

## Must state

- `handle` is a `pipe` property: every layer's function runs in composition
  order, and each one after the first receives the *previous layer's return
  value* as its argument — not the original request unmodified.
- The base layer (`chat.api.messages`'s own) runs first and returns `req`
  unchanged; the derived layer (`chat.api.logging`, which uses `chat.api`)
  receives that same value next, logs it, and returns it again.
- The whole call's result is the *last* layer's return value.
- All layers of a `pipe` property must agree in parameter and return shape —
  both layers here take and return `std.Request`.
