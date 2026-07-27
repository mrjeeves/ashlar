# 0031 — Observability was decided and never built

Date: 2026-07-27. Status: accepted.

## Context

A reviewer argued that Ashlar should be judged on five mechanical dimensions —
determinism, observability, repairability, change amplification, verification
cost — rather than on whether its nonlocal design *feels* complicated, and that
its real weaknesses are wherever it loses derivability, not wherever it loses
locality.

**ADR-0012 accepted that argument in full on 2026-07-23**, including the
sentence this ADR is about:

> Deterministic but silent behavioral change remains a failure; the toolchain
> must expose the semantic delta.

So the question was never whether the reviewer was right. It was whether the
repository had delivered what it had already agreed to. It had not, and the
reason is structural: ADR-0012 named four properties, and only **determinism**
had requirement ids behind it (C2, C6, F2). No requirement meant no test, and no
test meant no code — while `suites/coverage.md` reported every requirement
covered, truthfully, because the missing requirements were not requirements.

Three defects were found by driving the release binary, per requirements §2.
None was reachable from a stuck test.

**① One `use` line silently reorders execution.** Four spaces, `alpha` and
`zulu` both layering `base.Chain`. Adding `use zulu` to `alpha` flipped the
composition order:

| | manifest layer order | `GET /` over HTTP |
|---|---|---|
| before | `base, alpha, zulu` | `x\|base\|alpha\|zulu` |
| after | `base, zulu, alpha` | `x\|base\|zulu\|alpha` |

`ashlar check` exited 0 and printed nothing. Worse, the `W001` tie-break warning
that had been flagging the pair *disappeared* on the same edit — the only signal
that existed got quieter as the change landed.

**② `ashlar fix` silently changed which part a page renders.** A view calling
`el(Card, {}, [])` rendered `BASE-CARD`. A colliding `audit.Card` arrived
upstream, producing `E002`; running the toolchain's own repair rewrote the call
to `el(audit.Card, ...)` — `part_fulls[0]`, alphabetically first, not the one
the program had been resolving to — and the page rendered `AUDIT-CARD`. Clean
build before, clean build after, different program.

**D2 was satisfied the entire time.** That is the finding worth generalising:
D2 asks whether an applied fix *compiles*, never whether it *means the same
thing*. A fix that compiles and silently means something else is worse than no
fix, because it is applied without review and leaves nothing to review.

**③ The same ambiguity was repairable where a name was mentioned and not where
it was used.** `Card` bare got a machine edit; `Card.title` got a note. The
emission branched on `k == segs.len()`, and the note-only branch was the far
commoner position — names are usually used, not mentioned.

A fourth finding is about measurement rather than behavior. **`t_d5.rs` skipped
every fixture with no machine-applicable fix**, so the advertised mean
rounds-to-clean of 1.00 was computed over the fixtures that already had fixes.
When ② was repaired the corpus silently shrank from 11 cases to 10 and the mean
did not move. The true figure is **10 of 41 — 24%** of the diagnostic corpus
carries a machine-applicable fix, and the README had been describing 1.00 as
holding "over the whole error corpus."

## Decision

**C9 (new).** A change to the use graph that alters composition order is
reported. Where a previous build's derived state is available, an edit that
resequences any part's layers produces a diagnostic naming the part and its
order before and after.

**D6 (new).** A machine-applicable fix never changes what a name resolves to.
Where restoring the author's meaning is not derivable, the diagnostic carries a
note naming every candidate and no edits.

**D5 (revised).** The measurement covers the whole corpus, reporting the
machine-applicable *fraction* alongside the mean of those that have fixes.

The delivered mechanism is deliberately small, because the derived state was
already there and only the comparison was missing:

- `delta.rs` compares the current program against the previous
  `ashlar.manifest`, which has always recorded each part's layers in
  composition order. No new derivation, and no new parser — `eval::from_json`
  is how `foreign.rs` and `settings.rs` already read the project's JSON.
- `W002` (req `C9`) fires from `check_project`, beside the other two on-disk
  facts (`foreign.json`, `settings.json`). It is a **warning**: the new order
  is usually intended, and C9 asks that the author be told, not stopped.
- `ashlar delta` prints the full before/after and touches nothing.
- `E002` emits no edits, at both emission sites, in every syntactic position.

**The baseline is the prior manifest, and its absence is silent.** The manifest
is gitignored, so a fresh clone and a CI job have nothing to compare against and
say nothing. That is honest rather than convenient: the case this is built for
is an agent editing in a live working tree, where the previous build is sitting
right there. A committed baseline would make the check fire in CI at the cost of
a tracked artifact that must stay in sync — in tension with "the manifest is
derived, never hand-edited."

## Consequences

- The D5 number **got worse on purpose**: 24% is what was always true, and the
  gate now prints it every run. The floor is set low (20%) deliberately. It is
  not an aspiration to reach 100% — D6 forbids inventing an edit where choosing
  one would be a guess, so some diagnostics must stay judgment-required, and
  driving the fraction up by relaxing that is the regression the gate exists to
  make visible.
- `suites/t_a4/16-ambiguous` left the D5 corpus, as predicted, because its fix
  was a guess counted as a correction.
- The reference's §8 sample diagnostic showed `E002` carrying an edit. It never
  matched what the resolver emitted (three different cause phrasings existed,
  and the sample was a fourth), and it is now both correct and correct-by-
  construction, since the emitted shape is what D6 requires.
- §6 claimed field access is "checked at build time; unknown fields are compile
  errors." That is false for a value of shape `data`: `e.data.valeu` compiles
  clean and answers `none`. Since `e.data.value` is the documented event idiom
  in 18 places across 14 examples, banning it is wrong; the checker genuinely
  cannot know a runtime union's keys. So this is D3's second category —
  undetectable, and now **documented with the reason** rather than covered by a
  sentence asserting a check that does not happen.

## What this does not close

- **The reviewer's research programme.** Comparing Ashlar against
  selective-import, separate-ordering and fully-explicit variants at 10 to
  10,000 spaces would mean building three more languages. Not attempted; in the
  roadmap.
- **Scale evidence.** `t_f1.rs` builds 1,000 files as 100 star clusters of
  depth 1 — zero layers, zero collisions, zero merged properties, every closure
  at most `{std, hub}`. It is an honest test of single-file re-check latency and
  says nothing about the derived-state work the transitive-visibility hypothesis
  rests on. In the roadmap.
- **Diagnostics that scale with causes, not symptoms.** One upstream collision
  produced 12 identical `E002`s. Each site needs its own distinct edit, so 12
  diagnostics is 12 real corrections rather than 12 copies of one — but nothing
  reports the single cause and its extent. In the roadmap.
- **Visibility and ambiguity deltas.** `delta.rs` compares composition order
  only. Names entering or leaving a space's closure, and names that newly become
  ambiguous, are equally derivable — `SpaceInfo::closure` is computed on every
  build and never serialized — and are not yet reported.
