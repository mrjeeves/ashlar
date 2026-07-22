## Correct reading

`n` is a `state` property: runtime-mutable data that lives for the process,
initialized to `0`, distinct from a plain value property (a fixed build-time
fact). `bump` is a function property declared on the same part, `Counter`,
so its body may legally assign `n` (`n = n + 1`); other parts could only read
`n` or call `bump` to change it. Every read of `n` is reactive by design.

## Must state

- `state n` declares a state property: runtime-mutable data belonging to
  `Counter`'s process lifetime, distinct from a plain value property (which
  is a fixed build-time fact).
- Its initial value is `0`, required syntactically, since state properties
  must state an initial value.
- `bump`'s body (`n = n + 1`) is a legal assignment because `bump` is a
  function property declared on the same part that owns `n`; only such
  functions may assign it — other parts could only read `n` or call `bump`.
- Reads of `n` are reactive by the language's design: anything that reads a
  state property automatically observes later changes to it.
