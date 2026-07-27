# T-BROWSER — the corpus under a real browser

## Why this exists

Every other runtime suite drives Ashlar with an HTTP client this repo wrote,
and that client cannot find what a browser finds. It never asks for a
favicon, never reads a `<title>`, never reconnects a socket on reload, and
never types two people's keystrokes into one field at the same time.

Each check in `drive.mjs` was a defect when it was written:

| check | what it was |
|---|---|
| the index names its own tab | every page Ashlar served had a blank tab; the reference never mentioned `title` |
| an absolute path is answerable | `/robots.txt` and `/favicon.ico` were unreachable — `files` mounted a directory under a route prefix, and `/` was taken |
| the browser console is clean | every page load logged a 404 for the favicon nobody could serve |
| a view instance's state belongs to its page | G3 claimed hot reload "preserves process state" flatly; a live socket through a source edit took a counter 3 → 0 |
| two people typing at once both survive | the merge, driven by real keystroke timing rather than synthesized frames |

## Why it is not in CI

It needs a browser and node. `cargo test` must need neither — G1 is a single
binary with no install step, and the workspace has no external crates. So
this is a **hand-run gate with recorded results**, exactly like T-A3.
Playwright is resolved from the *operator's* directory, never from this repo.

## Running it

```
mkdir -p /tmp/ashlar-browser && cd /tmp/ashlar-browser
npm init -y && npm i playwright          # or PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 if a browser is already present
cargo build --release --manifest-path <repo>/Cargo.toml

node <repo>/suites/t_browser/drive.mjs \
     <repo>/target/release/ashlar \
     --root <repo> \
     --shots /tmp/ashlar-browser/shots
```

`ASHLAR_CHROMIUM=/path/to/chrome` picks a specific binary when Playwright's
own download is absent. `--shots` is optional and writes a PNG per example.

Exit status is 0 only when every check passes; the count is the last line.

## Pass definition

**Every check passes.** Unlike T-A3 there is no threshold: these are not
judgments about how a reader interprets a construct, they are facts about
what the program did in a browser. A failure is a defect or a stale check,
and either way the run is not green.

## Recording a run

Write the output to `results/<date>-<browser>-run<N>.md` with the binary's
commit, the browser version, and the full check list. A run that is not
recorded did not happen — the point of the gate is that the evidence
outlives the session that produced it.
