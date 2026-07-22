## Correct reading

`synced` declares mutable numeric state whose changes are additionally
synchronized/propagated beyond the local process — the word implies
distribution (to clients, peers, or observers), not just mutability.

## Must state

- `synced` declares runtime-mutable state (initial value 0), like a state
  variable.
- The word `synced` additionally implies the value is
  synchronized/propagated somewhere beyond a plain local variable —
  shared with clients, sessions, or observers.
- `online: number = 0` — numeric shape with initial value zero.
- `Room` is a part in space `chat.presence` declaring only this one
  property.
