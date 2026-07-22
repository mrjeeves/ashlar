## Correct reading

`append` on `tags` declares combining-by-concatenation: the two layers'
lists join rather than the later replacing the earlier. The combined
`tags` holds both `"core"` and `"extra"`.

## Must state

- `append` marks `tags` as combining across the two declarations: the
  lists concatenate; the later declaration does not simply replace the
  earlier one.
- The combined `tags` contains both `"core"` and `"extra"` (base's entries
  first).
- `tags` is a list of text (`[text]`).
- File 2's `part config.Config` targets the same `Config` declared in file
  1 (dotted name + `use config`), contributing to it rather than declaring
  a new part.
