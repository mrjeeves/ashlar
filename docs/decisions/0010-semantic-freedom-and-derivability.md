# 0010 — Semantic freedom and derivability

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
