# Language Requirements: An Agent-Authored Composition Language
*Requirements for clean-room implementation. Not instructions.*
---
## 0. What this document is
A set of requirements — statements of what must be true — for a programming language and runtime whose primary author is a machine and whose primary reader is a human reviewing the machine's work.
Requirements are numbered so tests can reference them. Every requirement is verifiable. Nothing here is procedural; where this document appears to say "do X," it has failed and should be rewritten as "X is true."
**The name of the language is not decided here.** Principle 2 says names matter more than anything. The most-repeated name in the system is the composable unit's keyword — the language's own name appears only in the CLI, the manifest header, the file extension, and diagnostics — so the guessability budget is spent on the unit keyword first.
---
## 1. The hierarchy
Four layers. Each serves the one above it. When two layers conflict, the higher one wins.
```
VISION          The principles in §2. Fixed. If the vision is wrong, stop.
REQUIREMENTS    This document. Revised when it fails to express the vision.
TESTS           The current best encoding of the requirements. Revised freely.
CODE            Whatever makes the tests pass.
```
This hierarchy exists to answer the only question that matters during implementation: *when something conflicts, which thing yields?* Code yields to tests. Tests yield to requirements. Requirements yield to the vision. Nothing overrides the vision.
Tests are not sacred. A test that passes while the requirement goes unmet is a broken test. A test that fails while the requirement is met is a broken test. Both get fixed against the requirement, not defended.
---
## 2. The method
For every unit of work, at every scale:
1. Identify which requirement the unit serves.
2. Write the test that would prove the requirement met.
3. Build until it passes.
4. If it cannot pass, the requirement is wrong or incomplete. Return to §1 and revise upward, not downward.
Step 4 is the one that matters. The failure mode is weakening a test to accommodate an implementation. The correct response to an unsatisfiable test is to question the requirement against the vision — and if the requirement is sound, the implementation is wrong no matter how much of it exists.
There is no phase in which tests are written and no phase in which they are not. The test suites have dependencies on each other (§9) and that induces a rough sequence, but sequence is a consequence of dependency, not a prescribed process.
---
## 3. Principles (the vision)
**Code is cheap, good design isn't.** Generation is nearly free. Verification, comprehension, and change are not. Optimize those; never ration generation.
**Names matter more than anything.** Names are the only binding mechanism. Not paths, not positions, not file locations, not declaration order.
**The build is state, the code is intent.** Source declares what should be true. The build computes where everything lives and how it resolves.
**Things should work similarly to other things in a way that makes sense.** Prefer resonance with what a reader already knows on the surface; prefer internal consistency in semantics. Where they conflict, resolve toward whichever fails loudly when guessed wrong.
**Refactoring is a first-class concern.** Changing intent must have computable blast radius. This is what makes cheap code trustworthy: if an agent generates ten times the volume a human reads, the unread portion is only safe when changes to it are provably contained.
The last two are one principle at two time scales. State derived at build time is what makes intent editable without fear.
---
## 4. Requirements: reference and surface
**A1.** The complete language reference is at most **40,000 characters**, measured as UTF-8 bytes of the canonical reference document.
**A2.** The reference is sufficient. No correct program requires knowledge not contained in it. No feature exists that the reference does not describe.
**A3.** A model with no reference in context, shown any construct from the language, states its meaning correctly. This is measured against a fixed corpus (§9, T-A3) with a defined pass threshold.
**A4.** Where a reader's guess is wrong, the wrongness surfaces as a compile error, not as running code with different behavior than intended. False familiarity is worse than unfamiliarity: unfamiliar syntax produces errors, familiar-but-different syntax produces bugs.
**A5.** No feature costs reference budget disproportionate to its value. A construct occupying 2,000 characters — 5% of the total — is worth 5% of the language or is removed.
**A6.** The language is not extensible at the surface level. No macros, no user-defined syntax, no operator overloading. Any such feature makes A2 unsatisfiable, because the reference can no longer describe what an arbitrary program means.
---
## 5. Requirements: names and binding
**B1.** Names are the only binding mechanism. No file path, argument position, declaration order, or file location affects what a name refers to.
**B2.** No name is dynamically constructed for any entity the compiler must reason about. Structural access by computed key is not available for composition, scope, or reference.
**B3.** Every name in scope resolves to exactly one definition. Zero resolutions and multiple resolutions are both compile errors.
**B4.** Two names in the same scope differing only by case or separator convention are a compile error. The compiler supplies naming discipline because the primary author does not reliably supply it.
**B5.** No source file contains a location. Not a path, not a URL, not a version. Locations exist only in the manifest.
**B6.** Names are namespaced by dotted identifier, not by directory structure. `chat.ui.Message` is a name; where it lives is a build fact.
**B7.** A dependency declaration brings every name that dependency provides into scope, transitively through the dependency graph. There is no import list, no aliasing, no destructuring form.
---
## 6. Requirements: composition and merge
**C1.** There is one composable unit. UI elements, routes, services, state stores, and data shapes are all the same kind of thing and compose by the same mechanism.
**C2.** Composition order is deterministic and computable from declarations alone. Given the same declarations, the flattened order is identical across runs, machines, and file layouts.
**C3.** Where flattening required an arbitrary tie-break — two sources unordered by any declared relationship — the compiler emits a warning naming both and suggesting the declaration that would order them.
**C4.** There are exactly **five** merge kinds. A sixth is added only by removing one.
| Kind | Behavior |
|---|---|
| *(default)* | Replace. Later definition wins entirely. |
| `append` | Lists concatenate, strings concatenate, maps merge one level. |
| `deep` | Maps merge recursively. Lists append. |
| `stack` | All implementations run in order. Returns merge onto the receiver. |
| `pipe` | All implementations run; each receives the previous return value. |
`stack` and `pipe` accept a `reverse` modifier running derived-to-base. This is the correct default for teardown and is why the modifier exists.
**C5.** Merge kind is declared at the property, inline with it, and is part of the property's identity. A composed source may not change an inherited property's merge kind. Attempting to is a compile error.
**C6.** Given two composed values and a merge kind, the result is fully determined. No merge outcome depends on runtime state.
**C7.** Omitting a merge kind means replace. The common case carries no ceremony.
**C8.** Lifecycle is not a distinct concept. It is `stack` plus a declared ordering of names, and the reference presents it that way.
---
## 7. Requirements: errors
**D1.** Every diagnostic identifies a location, states the cause in one sentence, and states the correction as an instruction specific enough to apply without judgment.
**D2.** **Applying a suggested correction produces source that compiles.** Where a diagnostic offers a machine-applicable fix, that fix resolves the error it was offered for and introduces no new one.
D2 is the requirement that converts "errors are corrections" from an aspiration into a fact. It is mechanically checkable and it is the highest-leverage requirement in this document.
**D3.** Every condition the runtime could detect is detected at compile time instead, or is documented in the reference as undetectable with the reason. There is no third category.
**D4.** Diagnostics are structured and machine-readable first, human-rendered second — the inverse of every existing compiler.
**D5.** The number of round trips from "agent writes code" to "code is correct" is the measure of the compiler's quality. Every diagnostic that is a correction removes a round trip.
---
## 8. Requirements: refactoring, build, runtime
### Refactoring
**E1.** Every refactor is a command issued to the toolchain, not a text edit. The compiler computes the change set from the manifest and applies it atomically.
**E2.** After a refactor completes, no stale reference to the prior state exists anywhere in the program. This is checkable by exhaustive search.
**E3.** Every refactor reports its complete blast radius before applying.
**E4.** Every refactor is atomically reversible. Forward then back yields byte-identical source.
**E5.** A refactor that cannot compute complete blast radius refuses to run and reports why. It never applies partially.
**E6.** The refactor command set is complete enough that editing text to refactor is never the easier path.
### Build
**F1.** Incremental compilation of a single-file change completes in under **100ms** in a project of 1,000 source files. This is not a performance nicety; it is the difference between an agent loop that verifies and one that guesses.
**F2.** The manifest is fully derivable from source. Deleting it and rebuilding produces an identical manifest.
**F3.** Relocating any source file changes no source content and produces a manifest identical except for recorded locations.
### Runtime
**G1.** A single binary. No install step, no runtime dependency resolution, no package manager, no registry.
**G2.** The same handler serves HTTP and WebSocket. Transport is not visible in handler code.
**G3.** Hot reload on source change preserves process state.
**G4.** The builtin set covers routing, request handling, persistence, reactive state along two axes — lifetime (in-memory or persisted) and scope (shared or per-user `owned`), with cross-client synchronization a property of the no-client-code architecture rather than a distinct class (ADR-0015) — authentication, authorization, file serving, background tasks, scheduled tasks, real-time channels, and structured logging. Everything else is foreign-function interface.
**G5.** The absence of a package registry is a requirement, not a gap. It removes version resolution, transitive conflict, and supply-chain surface — all pure cost to an agent author.
---
## 9. Test suites
Each suite proves specific requirements. The mapping is explicit so coverage is checkable.
**T-A1 — Reference size.** Counts UTF-8 bytes of the reference. Fails the build over 40,000. Runs on every commit. Trivial to write, and it is the requirement most likely to be quietly violated over time.
**T-A2 — Reference sufficiency.** Extracts every code example from the reference, compiles each, asserts success. Separately: asserts every language construct appearing in any test fixture also appears in the reference.
**T-A3 — Guessability.** A fixed corpus of snippets paired with their correct interpretation. A model with no reference in context is asked what each does. Agreement is scored against a defined threshold. Every failure is a design bug in the syntax, not a documentation gap.
This suite exists before the compiler does and is the primary gate on syntax decisions.
**T-A4 — Loud failure.** A corpus of plausible-but-wrong constructs — the things a model would write if it guessed from a neighboring language. Each must produce a compile error. Any that runs is an A4 violation.
**T-B — Resolution.** Given a dependency graph, asserts which names are visible where. Includes: transitive visibility, zero-resolution errors, multi-resolution errors, case/separator collision errors, and the assertion that no source fixture contains a path.
**T-C — Composition and merge.** Exhaustive matrix: five merge kinds × value shape combinations × two and three composed sources. Plus: flattening determinism across file layouts, tie-break warning emission, merge-kind-change rejection.
**T-D — Correction.** For each error class the compiler can emit: a broken fixture, an assertion that the diagnostic contains a correction, and — the essential part — **an assertion that applying the correction produces compiling source.** This suite is the proof of D2 and should be the largest in the project.
**T-E — Refactor.** For each command: blast radius correctness, post-refactor absence of the old state, roundtrip byte-identity, and refusal-on-incomplete-radius.
**T-F — Build.** Manifest determinism (delete and rebuild), relocation invariance (move files, diff manifest modulo locations), and incremental latency benchmarked as a hard-failing test.
**T-G — Runtime conformance.** Behavioral tests for each builtin. Protocol transparency: the same handler fixture exercised over HTTP and WebSocket must produce identical results.
**T-META — Coverage.** Parses this document for requirement identifiers, parses the test suites for requirement annotations, asserts every requirement has at least one test. A requirement with no test is not a requirement.
---
## 10. Non-goals
- **Not a general-purpose systems language.** It builds servers and interfaces.
- **Not human-optimized.** Where agent legibility and human ergonomics conflict, agent legibility wins.
- **Not backward compatible.** Foreign-function interface only.
- **Not extensible at the language level.** See A6.
- **Not a package ecosystem.** See G5.
---
## 11. Open decisions
Unresolved by design. Each requires judgment and each should be resolved by writing the test first.
**The name of the language.**
**The name of the composable unit.** It must survive T-A3. Candidates carry collisions: `component` (UI prior), `type` (type-system prior), `entity` (ECS/DDD prior), `model` (ML/MVC prior). The test decides, not preference.
**Typing discipline.** Structural, nominal, or gradual. Constrained by A1 and D1: a type system costing 3,000 characters of reference is unaffordable at any expressive benefit, and its errors must be corrections.
**Anonymity boundary.** Anonymous inline functions are hostile to E2 — they cannot be renamed, moved, or referenced. A precise rule is needed for what may be anonymous. Working position: anything reusable must be named, anything single-use may be inline. The boundary needs definition.
**Compilation target.** Bytecode or native. F1 is the binding constraint; strategy is not specified.
**Diagnostic wire format.** D4 specifies the ordering of concerns, not the encoding.
---
## 12. What is done first
Not a process. A consequence of test dependency.
T-A1 and T-A3 have no dependencies — they test the reference, which is written before any implementation exists. They are therefore first, and they gate everything: a reference that fails T-A1 means the language is too large, and a syntax that fails T-A3 means the surface is wrong. Neither is fixable by implementation.
T-A4 and T-B follow, requiring a parser and resolver but no runtime. T-C requires composition. T-D requires the full front end. T-E requires the manifest. T-F and T-G require the whole system.
The first artifact is therefore the reference under 40,000 characters, and the first test is the one that counts its bytes.

## Revisions

2026-07-22 — §0 corrected: the most-repeated name is the unit keyword, not the language name (see docs/decisions/0002-unit-part.md). Revised per §1: requirements are revised when they fail to express the vision.
