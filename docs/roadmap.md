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

**Capabilities with no corpus site.** An adversarial read listed ten. Four
now have one — the directory form of `files`, `log.debug`, `log.warn`, and
duration units beyond `ms`, all in `examples/ticker`. Still undefended:
one-argument `fail(message)` (all 27 call sites pass a status), `range()`,
`json()`, `peruser state` (only `peruser stored` is used), `pipe reverse`,
and the `native` and `http` foreign transports — those two were driven by
hand and work, so they are uncovered rather than broken. Nothing in the
corpus would notice any of them breaking. Serves **A2, G4**. Proven by:
examples that use them, and the driving tests that come with those.

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
