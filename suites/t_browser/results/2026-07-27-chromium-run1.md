# T-BROWSER run 1 — 2026-07-27

- binary: `target/release/ashlar` at 645b256 (+ working tree of this increment)
- browser: Chromium via Playwright 1.62.0, headless
- command: `node suites/t_browser/drive.mjs target/release/ashlar --root . --shots …`

## Result: 12/12

```
PASS  counter: a click patches the view in place, server-side  — clicks: 0 -> clicks: 1
PASS  counter: a view instance state belongs to its page (G3)  — after reload: clicks: 0
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

12/12 checks passed
```

## Notes

The first run of this gate was **10/12**: both console checks failed on a
404 for `/favicon.ico`. The capability to answer that path had landed, but
neither example had been given an icon to serve — the finding was closed in
the language and left open in the corpus. That is the gate earning its place
on its first run, and it is why the examples now ship a favicon.
