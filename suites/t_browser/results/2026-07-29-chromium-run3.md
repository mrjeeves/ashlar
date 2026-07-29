# T-BROWSER run 3 — 2026-07-29

- binary: `target/release/ashlar` at c14e9b2 (+ this increment)
- browser: Chromium 1194 via Playwright, headless
  (`ASHLAR_CHROMIUM=/opt/pw-browsers/chromium-1194/chrome-linux/chrome`)
- one new check, and two labels moved

The counter example now carries BOTH scopes a `state` property can have —
per-instance beside singleton — because the difference is invisible in one
window and that is the whole point of it. The two existing checks moved with
the button's label; the new one is the claim the example was rewritten to
make, and it can only be made with a second real tab open.

## Result: 19/19

```
PASS  counter: a click patches the view in place, server-side  — this window: 0 -> this window: 1
PASS  counter: a view instance state belongs to its page (G3)  — after reload: this window: 0
PASS  counter: shared state crosses tabs, per-instance state does not  — this window: 1 / everyone: 1 / other's own this window: 0
PASS  counter: the browser console is clean
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

19/19 checks passed
```

## Also driven by hand, outside the gate

The gallery, with all sixteen framed examples running under
`showcase/serve.sh`: every tile's frame loads its own server, the stage
carries the lead, and promoting a tile patches the stage without reloading the
page or disturbing the frames under it. Not added to `drive.mjs` because it
needs sixteen servers rather than one, which is the launcher's job and not a
check's.
