# T-BROWSER run 4 — 2026-07-30

- binary: `target/release/ashlar` at b9524c6 (+ this increment)
- browser: Chromium 1194 via Playwright, headless
  (`ASHLAR_CHROMIUM=/opt/pw-browsers/chromium-1194/chrome-linux/chrome`)
- five new checks, all on `enclave`

Sharing a file in the room was a slash command typed into the conversation, on
the excuse that a browser cannot hand a server a path it can open. True, and
beside the point: a picker hands over the FILE. What was missing was
`multipart/form-data` in the runtime.

Neither half of the replacement can be checked without a browser — a file
dialog and a `DataTransfer` do not exist otherwise — which is why all five
land here rather than in `t_examples`. The mesh node is absent, and that is
fine: what is under test is that the bytes leave the page.

## Result: 24/24

```
PASS  counter: a click patches the view in place, server-side  — this window: 0 -> this window: 1
PASS  counter: a view instance state belongs to its page (G3)  — after reload: this window: 0
PASS  counter: shared state crosses tabs, per-instance state does not  — this window: 1 / everyone: 1 / other's own this window: 0
PASS  counter: the browser console is clean
PASS  enclave: the shelf carries a real file picker  — 1 file input(s)
PASS  enclave: picking a file posts it as multipart, with no client code  — multipart/form-data
PASS  enclave: dragging over the form says so, for the stylesheet
PASS  enclave: dropping a file submits it, with nothing clicked  — multipart/form-data
PASS  enclave: the page does not grow and the composer stays put  — page fits, composer at 720 of 720
PASS  slate: the index names its own tab (§9.4)  — "slate"
PASS  slate: an absolute path is answerable (§9.8)  — 200 text/plain
PASS  slate: a native form post makes a pad  — http://127.0.0.1:8402/p/rims-not-wheels
PASS  slate: the pad names its tab after itself  — "Rims Not Wheels · slate"
PASS  slate: what one page types appears on the other  — "line one"
PASS  slate: two people typing at once both survive the merge  — "line one AAA BBB"
PASS  slate: two pages present, one per tab  — ["basalt 1","marble 2"]
PASS  slate: presence departs when a tab closes  — ["basalt 1"]
PASS  slate: the browser console is clean
PASS  slate: a live page is not marked offline  — false
PASS  slate: a page cut off notices and says so (§9.5)  — watchdog fired
PASS  slate: it reconnects when the network returns  — false
PASS  slate: different lines, nobody is warned  — "" / ""
PASS  slate: two carets on one line, each page names the other  — "marble 2 is on your line" / "basalt 1 is on your line"
PASS  slate: a departed page takes its caret with it  — ""

24/24 checks passed
```

## One trap for whoever runs this next

The gate has no port of its own per example, and a run that dies mid-way leaves
its server listening. An orphaned `enclave` on 8403 — from a crashed earlier
run — is what made the second slate block wait thirty seconds for a textarea on
a page that has none. `serve()` proves a port answers, not that the right
program is answering. Check for strays before believing a failure:

```
ps aux | grep '[a]shlar run examples'
```
