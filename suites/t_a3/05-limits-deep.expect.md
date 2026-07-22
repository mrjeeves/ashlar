## Correct reading

`deep` declares recursive map merging. The two nested `http` maps combine
key-by-key: the merged `limits.http` holds both `max: 10` and
`timeout: 30`.

## Must state

- `deep` marks `limits` as merging recursively: nested maps combine
  key-by-key rather than the later map wholesale replacing the earlier.
- The combined `limits.http` contains BOTH `max: 10` and `timeout: 30`.
- Both declarations target the same `Config` part (dotted name + `use`).
- The values are nested map literals (`{ http: { ... } }`).
