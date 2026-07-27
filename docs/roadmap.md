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

## Open

**Capabilities with no corpus site.** An adversarial read listed ten. Four
now have one — the directory form of `files`, `log.debug`, `log.warn`, and
duration units beyond `ms`, all in `examples/ticker`. Still undefended:
one-argument `fail(message)` (all 27 call sites pass a status), `range()`,
`json()`, `peruser state` (only `peruser stored` is used), `pipe reverse`,
and the `native` and `http` foreign transports — those two were driven by
hand and work, so they are uncovered rather than broken. Nothing in the
corpus would notice any of them breaking. Serves **A2, G4**. Proven by:
examples that use them, and the driving tests that come with those.

**Scale evidence tests no graph.** `t_f1.rs` builds its 1,000 files as 100 star
clusters of depth 1: 1,000 parts, zero layers, zero collisions, zero merged
properties, every closure at most `{std, hub}`. That is an honest test of
single-file re-check latency and says nothing about the derived-state work the
whole transitive-visibility trade rests on — closure size, layer flattening,
collision density. Serves **F1, C9**. Proven by: a generator with controlled
depth, fan-out, layer density and collision rate, and a measured gate on the
derived-state work rather than only on parse throughput (ADR-0012).

**The semantic delta covers order only.** `delta.rs` compares composition order.
Names entering or leaving a space's closure, and names that newly become
ambiguous, are equally derivable — `SpaceInfo::closure` is computed on every
build and never serialized — and are not reported. Serves **C9**. Proven by:
`ashlar delta` reporting visibility and ambiguity changes, with the driving test
that a widened `use` names what it made visible.

**Diagnostics multiply with symptoms.** One upstream collision produced 12
identical `E002`s. Each site does need its own distinct edit, so this is not 12
copies of one diagnostic — but nothing reports the single cause and its extent,
which is what an agent needs to decide whether to qualify twelve references or
rename one part. Serves **D5**. Proven by: a cause-level report naming the
collision once with its downstream sites, and a rounds-to-clean measurement over
a multi-site fixture.

## Standing method notes

Not open items — neither has work waiting behind it.

**A3 is met by measurement, and can now only be re-measured from outside.**
Run 5 scored 24/25 against a bar of 20/25. It was a *reduced-contamination* run,
not a cold one — an in-repo reader received the project instructions before any
prompt existed to forbid them. That category is now gone rather than improved:
`AGENTS.md` carries the reference itself, so in-repo contamination is total and
certain, and `suites/t_a3/PROTOCOL.md`'s outside-the-repository run is the only
one there is (docs/decisions/0021-the-a3-readers-were-not-cold.md). The standing
24/25 is the last figure takeable under the old arrangement, not one that can be
re-taken on demand.

**Two A3 findings stand recorded with no change to make.** *A3-F1*:
cross-file layering does not cold-read — 0 of 11 readers across a 2×2 over
the merge-kind word and its position, so it is the construct and not the
word. *A3-F7*: `{text: number}` read as a one-field record rather than a map.
Both are in `suites/t_a3/results/` with the runs that found them.
