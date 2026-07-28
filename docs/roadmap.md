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

**A3 needs its baseline-aware run.** Runs 1–5 measured whether readers without
the complete reference could infer the language. A3 now measures the actual
authoring environment: a fresh in-repository agent receives `AGENTS.md` and is
shown one fixture without its rubric or prior answers. No recorded run used
that exact baseline after the reference moved into the contract. Serves **A3**.
Proven by: a 25-fixture run under `suites/t_a3/PROTOCOL.md`, recorded with the
agent surface and revision it used.

**The mesh is proven on one box, not across two.** `examples/enclave` ships no
binding, so it reaches the mesh node on the machine that runs it; `t_examples`
drives the whole contract against a stand-in bound into the staged copy, and
two full stacks on one box — two identities, two published sites — have been
driven end to end: each saw the other in its roster and rendered the other's
site as a link its node had mapped locally. What one box cannot show is the
part the network decides: signaling between two real machines, ICE across a
NAT, and a roster losing a node that actually went away. Serves **B5, G4**.
Proven by: a hand-run gate on two machines, recorded like T-BROWSER's — a
second node's site opened from the first, and `ashlar mesh` agreeing with the
node on both. The Windows half of the socket rides the same gate: a named pipe
is opened here with `std` alone and has never been driven against a real node.

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

**Two A3 findings stand recorded with no change to make.** *A3-F1*:
cross-file layering does not cold-read — 0 of 11 readers across a 2×2 over
the merge-kind word and its position, so it is the construct and not the
word. *A3-F7*: `{text: number}` read as a one-field record rather than a map.
Both are in `suites/t_a3/results/` with the runs that found them.
