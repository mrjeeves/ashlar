## Correct reading

`foreign fetch: (url: text) -> data` declares a function implemented outside
Ashlar, callable as `fetch(url)` from any code that can see the `net` space,
taking `text` and returning `data`. The build binds `fetch` to a host
library the manifest records; nothing about that location appears in this
source. Arguments and the return value cross the boundary as data and are
shape-checked at runtime, at the call site; a mismatch there is a runtime
fault, not a build error. Foreign calls may block; the runtime schedules
around them.

## Must state

- `foreign fetch: (url: text) -> data` declares a function implemented
  outside Ashlar, callable from Ashlar code as `fetch(url)`, taking `text`
  and returning `data`.
- The build binds `fetch` to a host library at a location the manifest
  records — nothing about that location appears in this source file.
- Arguments and the return value cross the foreign boundary as data and are
  shape-checked at runtime at the call site; a mismatch there is a runtime
  fault, not a build-time error.
- Foreign calls may block; the runtime schedules around them, so calling
  `fetch` needs no special syntax (no `await`, no callback) at the call
  site.
