## Correct reading

`Config.limits` is declared with merge kind `deep` in `config`, holding
`{ http: { max: 10 } }`. `config.ext` uses `config` and layers `Config` with
`limits deep = { http: { timeout: 30 } }`, restating the same kind. `deep`
merges maps recursively at every depth, so the nested `http` maps combine
key-by-key rather than one replacing the other: the composed `limits` is
`{ http: { max: 10, timeout: 30 } }`.

## Must state

- `deep` merges maps recursively at every depth (unlike `append`, which only
  merges one level for maps) — nested maps combine key-by-key rather than
  one layer's map wholesale replacing the other's.
- The composed `limits` is `{ http: { max: 10, timeout: 30 } }`: both the
  base layer's `http.max` and the derived layer's `http.timeout` survive
  because the merge recurses into the nested `http` map.
- Every layer touching `limits` must restate `deep`; a layer stating a
  different kind, or omitting the kind, would be a compile error.
- The merge is computed at build time from the layered literals,
  deterministically, in use/composition order.
