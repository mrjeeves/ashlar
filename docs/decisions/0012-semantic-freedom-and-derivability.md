# 0012 — Semantic freedom and derivability

Date: 2026-07-23. Status: accepted.

## Context

Most programming languages are designed around human constraints. They
favor local reasoning, flexible expression, familiar syntax, and
abstractions that reduce what a person must hold in mind. Those are
sensible priorities when humans are the primary authors. Ashlar begins
from a different premise: generating code is inexpensive, while verifying
its meaning and changing it safely are costly.

The relevant trade is therefore not simplicity against complexity. It is
**semantic freedom against derivability**.

Semantic freedom is the number of valid ways an author can express or
interpret an intention. Aliases, implicit precedence, configurable
imports, operator overloading, user-defined syntax, dynamic lookup, and
multiple composition mechanisms all increase that freedom. They may make
an individual expression convenient, but they enlarge the space of
meanings the author, compiler, reviewer, and refactoring tools must
consider.

Derivability is the opposing quality. A property is derivable when the
toolchain can compute it completely from named declarations: what a name
denotes, which implementation runs, why it runs in that position, what a
change will affect, and which edits will restore correctness.

## Decision

Ashlar minimizes semantic freedom in order to maximize the derivability of
intent, behavior, and change.

This does not require every effect to be local or small. A change may
reach hundreds of files and still be safe when its complete semantic delta
can be calculated, explained, applied atomically, and verified. A small
change is unsafe when its consequences depend on convention, hidden state,
or unrecorded judgment.

Human cognitive difficulty is not by itself a reason to reject nonlocal
behavior. Agents can traverse large graphs, inspect manifests, and apply
extensive mechanical edits without fatigue. Their constraints are finite
context, incomplete retrieval, stale state, probabilistic inference, and
coordination across concurrent work. Ashlar should be evaluated against
those constraints rather than inherited intuitions about what feels
simple to a human author.

Determinism is necessary but not sufficient:

- **Determinism:** the same declarations produce the same result.
- **Observability:** the toolchain explains how that result was derived.
- **Stability:** a change in behavior appears as an explicit semantic
  delta, even when the new behavior is deterministic.
- **Repairability:** an inconsistency has a correction that requires no
  unrecorded design choice.

Broad transitive visibility and composition order derived from `use` are
therefore not defects merely because they have nonlocal consequences.
They are a testable trade: greater change amplification in exchange for
fewer declarations and fewer independent mechanisms. The trade succeeds
when the consequences remain fully observable and the cost of correction
remains bounded.

## Research questions

Ashlar's design should be tested against language variants and projects of
increasing size, dependency depth, fan-out, layer density, and name
collision rate. The primary measurements are compile-to-clean rounds,
tokens consumed, elapsed time, files inspected, semantic regressions,
blast-radius accuracy, and the proportion of corrections that can be
applied without judgment.

The questions are:

1. Does reducing semantic freedom lower agent error rates and
   compile-to-clean rounds?
2. Do whole-space visibility and derived composition order outperform
   selective imports and separately declared order?
3. Does correction cost remain bounded as dependency graphs grow?
4. Can agents complete changes by retrieving derived explanations rather
   than loading the entire program into context?
5. Can every behavioral change be represented as a complete,
   machine-readable semantic diff?
6. Do diagnostics report upstream causes rather than multiplying
   downstream symptoms?
7. Can concurrent agent changes either compose deterministically or fail
   with a complete correction?
8. Which dynamic boundaries introduce uncertainty that the compiler
   cannot derive away?
9. At what point does change amplification cost more than eliminating
   authoring decisions saves?
10. Which forms of semantic freedom provide expressive value rather than
    merely alternative spellings?

Human review remains important as a secondary measure: not as the primary
constraint on the language, but as a test that the compiler's derivation
is auditable.

## Consequences

- Language proposals are judged by how many new semantic choices they
  introduce and how completely their consequences can be derived.
- Nonlocal behavior is acceptable when its complete effect is computable,
  observable, and mechanically repairable.
- Deterministic but silent behavioral change remains a failure; the
  toolchain must expose the semantic delta.
- Large blast radius is a measured cost, not an automatic rejection.
- Agent performance under controlled change tasks is the primary evidence
  for or against the trade.

The governing principle is:

> An agent-authored language should minimize semantic freedom while
> maximizing the derivability of intent, behavior, and change.

## Resolution (2026-07-27) — observability was decided here and not built

This ADR named four properties and only **determinism** had requirement ids
behind it (C2, C6, F2). No requirement meant no test, which meant no code, while
`suites/coverage.md` truthfully reported every requirement covered — because the
missing ones were not requirements. Three defects lived under a green suite,
each found by driving the release binary, none reachable from a stuck test.

**① A `use` edge silently reordered execution.** Adding `use zulu` to `alpha`
flipped `base.Chain` from `base,alpha,zulu` to `base,zulu,alpha`; over HTTP the
program answered `x|base|zulu|alpha` where it had answered `x|base|alpha|zulu`.
`check` exited 0 and printed nothing — and the `W001` tie-break warning that had
been flagging the pair *disappeared on the same edit*, so the only signal that
existed got quieter as the change landed. That is precisely the failure the
Consequences above forbid.

**② `ashlar fix` silently changed which part a page renders.** `el(Card, ...)`
rendered `base.Card`; a colliding `audit.Card` arrived, `E002` fired, and the
toolchain's own repair rewrote the call to `audit.Card` — alphabetically first,
not what the program had been resolving to. Clean build before, clean build
after, different program. **D2 was satisfied throughout**, which generalises:
D2 asks whether a fix *compiles*, never whether it *means the same thing*.

**③** The same ambiguity was machine-fixable where a name was mentioned and not
where it was used, because emission branched on `k == segs.len()` — and the
note-only branch was the far commoner position.

**④** And a measurement failure: `t_d5` skipped every fixture with no machine
fix, so the advertised mean of 1.00 covered only fixtures that already had
fixes. Repairing ② shrank that corpus 11 → 10 and the mean did not move. The
true figure is 10 of 41 — **24%**.

**Decided:** **C9** (a change to the use graph that alters composition order is
reported), **D6** (a machine-applicable fix never changes what a name resolves
to), and D5 revised to measure the whole corpus. Delivered as `delta.rs`
comparing against the previous manifest — which has always recorded layer order,
so nothing new is derived and no parser is added — plus `W002`, `ashlar delta`,
and `E002` emitting no edits at either site. No manifest means no baseline and
nothing is claimed: the manifest is gitignored, so CI and fresh clones stay
quiet, and the case this catches is an agent editing in a live tree.

The D5 number got worse on purpose and prints every run. Its floor is set low
deliberately: D6 forbids inventing an edit where choosing one would be a guess,
so driving the fraction up by relaxing that is the regression the gate exists to
show.

**Still open, in `docs/roadmap.md`:** the research questions above remain
research questions — the variant comparison would mean building three more
languages; `t_f1`'s 1,000-file fixture has no graph at all (depth 1, zero
layers, zero collisions), so nothing measures the derived-state work this trade
rests on; diagnostics still multiply with symptoms rather than naming one cause;
and the delta covers order only, not visibility or ambiguity.
