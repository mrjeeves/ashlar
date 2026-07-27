# Roadmap

The open ledger. What is not yet true, what requirement it serves, and the
test that will prove it — nothing else. An empty open section is a claim, so
it is kept honest one way: an item leaves only when its test runs for real.

This page deliberately does not keep a record of delivered work. That record
exists twice already and better: `suites/coverage.md` maps every requirement
to the test that proves it, and T-META fails if that map lies in either
direction; `docs/decisions/` holds what was decided and why, and `git log`
holds what changed and when. A third prose copy went stale between the other
two, and a ledger nobody can trust is worse than none.

## Open — one item

**A program cannot answer `/favicon.ico`.** Every real browser requests it on
every page load, and every Ashlar program answers 404 — which Chromium logs as
a console error on a page that is otherwise clean. `files` (§9.8) serves a
DIRECTORY under a part's `route` prefix, so there is no way to put one file at
one absolute path, and the root route is already the program's own page. Found
by driving `examples/counter` and `examples/slate` with a real browser; the
repo's own client never asked for it. Serves **G4** (the builtin set covers
file serving) and **A4** (the wrongness surfaces as noise in every browser
console rather than as anything the author can act on). Proven by: a driven
example serving its own icon at the root with a 200, and a T-G case pinning
whatever rule is chosen — the honest options are letting `files` name a single
file, or the runtime answering 204 for an icon no program claims, and neither
has been argued yet.

## Standing method notes

Not open items — neither has work waiting behind it.

**A3 is met by measurement.** Run 5 scored 24/25 against a bar of 20/25, on
runs labelled *reduced-contamination* rather than cold: an in-repo reader
receives the project instructions before any prompt exists to forbid them,
which is measured per run and recorded. `suites/t_a3/PROTOCOL.md` carries the
instructions for running a provably cold one from outside this working
directory. That is a stronger proof of the same claim, not an unmet
requirement.

**Two A3 findings stand recorded with no change to make.** *A3-F1*:
cross-file layering does not cold-read — 0 of 11 readers across a 2×2 over
the merge-kind word and its position, so it is the construct and not the
word. *A3-F7*: `{text: number}` read as a one-field record rather than a map.
Both are in `suites/t_a3/results/` with the runs that found them.
