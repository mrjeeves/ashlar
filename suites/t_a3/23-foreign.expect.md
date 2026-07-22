## Correct reading

`foreign` declares a function implemented outside the language: `fetch`
takes a text URL and returns `data`, with no body in this file — only
the signature is declared here.

## Must state

- `foreign` marks `fetch` as implemented OUTSIDE this language — an
  external/native function bound elsewhere; only its signature appears
  here.
- `fetch` takes one parameter `url: text` and returns `data`.
- `data` reads as a generic/dynamic payload shape rather than one
  specific structured type.
- This is a declaration, not a definition — no function body exists in
  this file, and code in this space can call `fetch(...)`.
