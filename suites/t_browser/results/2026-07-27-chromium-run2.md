# T-BROWSER run 2 — 2026-07-27

- binary: `target/release/ashlar` at d0f22ef (+ this increment)
- browser: Chromium via Playwright 1.62.0, headless
- three new checks: socket liveness, reported from a live session

## Result: 15/15

```
PASS  counter: a click patches the view in place, server-side  — clicks: 0 -> clicks: 1
PASS  counter: a view instance state belongs to its page (G3)  — after reload: clicks: 0
PASS  counter: the browser console is clean
PASS  slate: the index names its own tab (§9.4)  — "slate"
PASS  slate: an absolute path is answerable (§9.8)  — 200 text/plain
PASS  slate: a native form post makes a pad  — http://127.0.0.1:8402/p/rims-not-wheels
PASS  slate: the pad names its tab after itself  — "Rims Not Wheels · slate"
PASS  slate: what one page types appears on the other  — "line one AAA BBBline one"
PASS  slate: two people typing at once both survive the merge  — "line one AAA BBBline one AAA BBB"
PASS  slate: two pages present, one per tab  — ["basalt 1","marble 2"]
PASS  slate: presence departs when a tab closes  — ["basalt 1"]
PASS  slate: the browser console is clean
PASS  slate: a live page is not marked offline  — false
PASS  slate: a page cut off notices and says so (§9.5)  — watchdog fired
PASS  slate: it reconnects when the network returns  — false

15/15 checks passed
```

## Notes

The three liveness checks exist because of a report from someone using slate
by hand: *"connection state is silent and deadly."* A socket had died without
either end being told — the page went on looking live, accepting typing, and
showing nothing new until it was reloaded.

The middle check cuts the page's network with the socket left half-open,
which is what a NAT timeout or a proxy reset actually does, and waits out the
watchdog. Before this increment it would have sat there indefinitely with no
sign; it now marks itself and recovers when the network returns.
