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

## Open — four items

All four came from an adversarial read of a finished increment, which is this
repo's stated practice and had not been run for several increments. Each is
reproduced; none is a guess.

**`ashlar run` never calls `stop`.** §9.1 says "On shutdown it calls `stop`,
then flushes stored state." There is no signal handling anywhere in
`crates/ashlar/src`, and nothing outside the test suite ever sets the stop
flag: a program logging in its `stop` stack logs nothing on SIGTERM or
SIGINT. Stored state survives regardless — it is flushed each tick when
dirty — so the loss is the `stop` stack itself, which is where a program
closes what it opened. Serves **G1, G3**. Proven by: a driven test that
signals a running server and asserts its `stop` stack ran.

**A routed part with no `handle` and no `view` answers 200 with a JSON dump
of `std.Request`** — headers included. `part bare { route = "/bare" }`
compiles clean and serves the request back. §9.2 documents no such response
form, which makes it surface the reference does not define, and rule 4 says a
construct that does not fully work does not exist. The same fallthrough
answers `/robots.txt/` — the trailing-slash form of a single-file route,
which `match_route` treats as the same route but `try_serve_files` compares
byte-wise. Serves **A2, G4**. Proven by: a T-A4 fixture for the bare part and
a T-G case for the trailing slash.

**`every` is only checked when it is text.** §9.7 says the duration is
checked at build time; `every = 10` passes `ashlar check` with no diagnostic
and yields a task that never runs. Silent, and exactly the D3 third category.
Serves **D3, G4**. Proven by: a T-A4 fixture.

**Capabilities with no corpus site.** The directory form of `files`,
`log.debug`/`warn`/`error`, one-argument `fail`, `range`, `json`,
`peruser state`, the `native` and `http` foreign transports, `pipe reverse`,
and the `m`/`h`/`d` duration units are all unused by every example — so
nothing in the corpus would notice them breaking. The transports were driven
by hand and work; the rest are simply undefended. Serves **A2, G4**. Proven
by: examples that use them, and the driving tests that come with those.

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
