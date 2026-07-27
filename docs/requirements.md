# Language Requirements: An Agent-Authored Composition Language
*Requirements for clean-room implementation. Not instructions.*
---
## 0. What this document is
A set of requirements — statements of what must be true — for a programming language and runtime whose primary author is a machine and whose primary reader is a human reviewing the machine's work.

These requirements distinguish the language from its first runtime target.
**Ashlar is an AI-first composition language. Its current runtime builds
servers and interfaces.** That is delivered scope, not the language's identity
or a permanent boundary on what its composition model may target. A future
runtime target is not implied by the current implementation: it earns its own
requirements and executable proof before it exists.

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
**Nor is this document sacred.** The hierarchy is a grant of authority as much as a tie-breaker: exactly one layer — the vision — is fixed and outside the implementer's reach. Every layer below it, this document included, is revisable by whoever is doing the work, on one condition: the revision serves the layer above it and arrives with the evidence and tests that show it does. Waiting for permission to revise a requirement that demonstrably fails the vision is the same failure as quietly weakening a test to spare the code — both replace the argument from above with someone's comfort. Where the deciding evidence cannot be obtained, say what is missing; do not guess and call it a decision.
---
## 2. The method
Two loops. The inner one builds; the outer one is where the requirements come from.

**Inner — for every unit of work, at every scale:**
1. Identify which requirement the unit serves.
2. Write the test that would prove the requirement met.
3. Build until it passes.
4. If it cannot pass, the requirement is wrong or incomplete. Return to §1 and revise upward, not downward.

Step 4 is the one that matters. The failure mode is weakening a test to accommodate an implementation. The correct response to an unsatisfiable test is to question the requirement against the vision — and if the requirement is sound, the implementation is wrong no matter how much of it exists.

There is no phase in which tests are written and no phase in which they are not. The test suites have dependencies on each other (§9) and that induces a rough sequence, but sequence is a consequence of dependency, not a prescribed process.

**Outer — the language is refined by running it against a world that does not cooperate:**
1. Build something real with it and **execute it** — a whole program, driven end to end over the transports it actually serves, fed the input an indifferent world actually sends.
2. Read what execution produced: what was rejected that should not have been, what ran that should not have, what the short program did versus what the correct program had to cost.
3. That evidence revises the requirements, per §1. Then the engine is refined to the revised requirement, with the test that pins it.

The outer loop is not a nicety and it is not the inner loop restated. A stuck test only reports that a requirement is unreachable *as specified*; it cannot report a requirement that is wrong while perfectly satisfiable, and those are the expensive ones. Only execution against uncooperative input finds them: a checker that rejects a correct program with a correction its author already applied, a boundary a program cannot ask about, a formatter that moves a comment onto the wrong declaration, a refusal that costs three times what inventing a value costs. Each of those passed every test it had.

So **exposure is a requirement of the method, not a testing tactic**: an example that quietly avoids the input it cannot handle is not an example, and a gate whose contamination is assumed rather than measured is not a gate. Where evidence cannot be obtained — a reader outside this repository, a machine that is not this one — say what is missing rather than guessing (§1). What execution finds lands as a revision here first, and only then as code.

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
**B5.** No source file **binds** by location. A name never resolves through a path, URL, or version, and no location is written as a literal the build would resolve — those live in the manifest, which the build derives. A program may still *depend* on a location it cannot know when it is written: it declares a named, shaped **setting**, and deployment supplies the value. The name is source, so B1 holds and the toolchain tracks it; the value is a deployment fact, so the build still owns where things live; and a missing or ill-shaped setting stops the program at startup, naming every gap at once, never surfacing as the first request that needed it.
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
**E4.** Every refactor is atomically reversible: the toolchain undoes it with another command, and the undone program **means** what the program that preceded it meant — the same parts with the same homes, the same composition order, and every name resolving to the same definition. A refactor may leave behind a declaration it reported adding: a widened `use` graph cannot silently change what an existing name resolves to, because a widening that did would be an ambiguity error (B3) that the refactor's own post-verify refuses. What it may never do is change what the program means. Byte-identical source on reversal is a stronger guarantee, which renaming in place does provide and which is preserved wherever it holds; it is not required of every refactor. Requiring it universally would force a refactor that must add a declaration to stay correct to refuse work it can do correctly, which costs E6 more than the stronger guarantee is worth.
**E5.** A refactor that cannot compute complete blast radius refuses to run and reports why. It never applies partially.
**E6.** The refactor command set is complete enough that editing text to refactor is never the easier path.
### Build
**F1.** Incremental compilation of a single-file change completes in under **100ms** in a project of 1,000 source files. This is not a performance nicety; it is the difference between an agent loop that verifies and one that guesses.
**F2.** The manifest is fully derivable from source. Deleting it and rebuilding produces an identical manifest.
**F3.** Relocating any source file changes no source content and produces a manifest identical except for recorded locations.
### Runtime
**G1.** A single binary. No install step, no runtime dependency resolution, no package manager, no registry.
**G2.** The same handler serves HTTP and WebSocket. Transport is not visible in handler code.
**G3.** Hot reload on source change rebuilds and reloads the program in place, carrying **state properties over by full name**, and open pages reconnect and re-render themselves against the new code. State that is not addressable by full name — a view instance's `state`, which belongs to a page (§9.4) — is reborn at its default, because the page it belonged to is gone. A change that fails to compile leaves the old program running.
**G4.** The builtin set covers routing, request handling, persistence, reactive state along two axes — lifetime (in-memory or persisted) and scope (shared or per-user `peruser`), with cross-client synchronization a property of the no-client-code architecture rather than a distinct class (ADR-0015, renamed by ADR-0019) — authentication, authorization, file serving, background tasks, scheduled tasks, real-time channels, and structured logging. Everything else is foreign-function interface.
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
**T-C — Composition and merge.** The matrix — five merge kinds × value shape combinations × two and three composed sources — plus flattening determinism across file layouts, tie-break warning emission, and merge-kind-change rejection. It never became a separate binary: composition is proved where it lives, in `compose.rs` and `resolve.rs`, and `suites/coverage.md` maps each C requirement to the test that runs. A suite named in this document that does not exist on disk is the coverage lie T-META now checks for.
**T-D — Correction.** For each error class the compiler can emit: a broken fixture, an assertion that the diagnostic contains a correction, and — the essential part — **an assertion that applying the correction produces compiling source.** This suite is the proof of D2 and should be the largest in the project.
**T-E — Refactor.** For each command: blast radius correctness, post-refactor absence of the old state, roundtrip reversal — byte-identity for the commands that guarantee it, structural identity of the program for every command — and refusal-on-incomplete-radius.
**T-F — Build.** Manifest determinism (delete and rebuild), relocation invariance (move files, diff manifest modulo locations), and incremental latency benchmarked as a hard-failing test.
**T-G — Runtime conformance.** Behavioral tests for each builtin. Protocol transparency: the same handler fixture exercised over HTTP and WebSocket must produce identical results.
**T-BROWSER — The corpus under a real browser.** The same examples `t_examples` drives, driven instead by an actual browser: titles, absolute-path assets, console cleanliness, a socket through a reload, and two people typing into one field at once. Hand-run with recorded results (`suites/t_browser/`), never in CI, because it needs a browser and node and G1 says `cargo test` needs neither. Its checks are facts about what the program did, so the bar is every check, not a threshold.

**T-META — Coverage.** Parses this document for requirement identifiers, parses the test suites for requirement annotations, asserts every requirement has at least one test. A requirement with no test is not a requirement.
---
## 10. Non-goals
- **Not a claim of a general-purpose systems runtime.** The current runtime
  builds servers and interfaces. That target states what is delivered now; it
  does not define Ashlar itself or foreclose another deliberately specified
  and verified runtime target (ADR-0030).
- **Not human-optimized.** Where agent legibility and human ergonomics conflict, agent legibility wins.
- **Not backward compatible.** Foreign-function interface only.
- **Not extensible at the language level.** See A6.
- **Not a package ecosystem.** See G5.
---
## 11. Decisions
Every decision this section once listed as open is resolved, each by a record in [`decisions/`](decisions/) stating what was decided, on what evidence, and what it cost. They are named here because the constraints they were decided against are still binding:

| was open | resolved |
|---|---|
| The name of the language | `ashlar` (0001) |
| The name of the composable unit | `part`, chosen against the guessability corpus rather than preference (0002) |
| Typing discipline | 0004, under A1 and D1: a type system whose errors are not corrections is unaffordable |
| Anonymity boundary | a function is either named or handed over (0005, 0022) |
| Compilation target | 0007, with F1 the binding constraint |
| Diagnostic wire format | 0006 |

A decision is reopened by evidence, not by preference, and reversing one is ordinary work (§1). New evidence outranks an earlier record, which was made on the evidence then available.

## 12. What is done first
Not a process. A consequence of test dependency.
T-A1 and T-A3 have no dependencies — they test the reference, which is written before any implementation exists. They are therefore first, and they gate everything: a reference that fails T-A1 means the language is too large, and a syntax that fails T-A3 means the surface is wrong. Neither is fixable by implementation.
T-A4 and T-B follow, requiring a parser and resolver but no runtime. T-C requires composition. T-D requires the full front end. T-E requires the manifest. T-F and T-G require the whole system.
The first artifact is therefore the reference under 40,000 characters, and the first test is the one that counts its bytes.

## Revisions

2026-07-22 — §0 corrected: the most-repeated name is the unit keyword, not the language name (see docs/decisions/0002-unit-part.md). Revised per §1: requirements are revised when they fail to express the vision.

2026-07-25 — **B5 revised** from "no source file contains a location" to "no source file binds by location," adding the `setting` construct (see docs/decisions/0020-settings-and-what-b5-actually-forbids.md). The vision's claim is that *names* are the only binding mechanism and that the build computes where things live; B5 had hardened that into a ban on a location appearing anywhere in source, which is a strictly stronger and different rule. The cost was not theoretical: with no file I/O in `std` and no way to name an address, a program could not render a list of links without a `foreign` co-process — a path to failure the requirement invented rather than the vision. Settings restore the capability without weakening the principle: the name binds, the value is deployment's, and absence fails loud at startup. Revised per §1: requirements are revised when they fail to express the vision.

2026-07-25 — **E4 revised** from byte-identical reversal to reversal of the program, with byte-identity kept as a property of the commands that have it (see docs/decisions/0018-reversibility-is-a-property-not-a-law.md). The vision asks that changing intent have computable blast radius and that changes be *provably contained*; it never asks that reversal be byte-exact. Byte-identity was a proof mechanism promoted to a law, and as a law it outranked E6: `move` already had to be carved out as an exception (ADR-0009), and any future refactor needing to add a declaration would have had to refuse correct work to preserve it. T-E's byte-identity assertions are all retained — the guarantee is weaker as a requirement and unchanged as a delivered fact. Revised per §1: requirements are revised when they fail to express the vision.

2026-07-27 — **§2 revised** to state both loops. The method described only the inner one: identify a requirement, write its test, build, and revise the requirement if the test cannot pass. That is a true account of how code gets written here and a false account of where the requirements came from. Every expensive finding this project has had arrived the other way — from executing a whole program against input that did not cooperate, not from a test that would not go green. A `pipe` layer rejecting a correct literal with a correction its author had applied, a boundary with no way to ask what arrived, a formatter handing a comment to the next declaration, a refusal costing three times what invention costs: each of those passed every test it had, and none of them was reachable from a stuck test, because a stuck test can only report a requirement that is unsatisfiable — never one that is wrong and perfectly satisfiable. Exposure is therefore part of the method and not a testing tactic. Revised per §1: requirements are revised when they fail to express the vision.

2026-07-27 — **§11 rewritten** from "Open decisions" to the resolutions. All six had been settled by ADRs 0001–0007 and 0022 while the section still told a reader the language had no name and no compilation target. A load-bearing document that is stale is worse than one that is silent: it is confidently wrong, which is the failure D1 forbids in diagnostics and A4 forbids in syntax, applied to the requirements themselves. **§9's T-C entry** corrected in the same pass: it described a test binary that was never built, because composition is proved in `compose.rs` and `resolve.rs` where it lives.

2026-07-27 — **G3 revised** from "hot reload on source change preserves process state" to what the runtime does and the reference already described: state carries over **by full name**, and open pages reconnect and re-render against the new code, so a view instance's `state` — which belongs to a page — is reborn at its default. Found by the outer loop (§2), not by a stuck test: a real browser held a live socket on `examples/counter`, the source was edited underneath it, and the count went 3 → 0 while the socket reconnected. `reference/ashlar.md` §9.1 has said "state properties carry over by full name" all along; G3's one-line summary was broader than the reference, the code, and the test that proved it, and a reader of the requirements would have predicted the 3. The requirement was wrong and the delivered behavior was right. `t_g3_a_view_instances_state_does_not_survive_reload` now pins the half that was unproved, beside the sibling that proves the other. Revised per §1: requirements are revised when they fail to express the vision.

2026-07-27 — **§0 and §10 revised** to distinguish Ashlar from its first
runtime target (ADR-0030). The vision defines an AI-first composition
language; none of its principles makes servers and interfaces the language's
identity. The old non-goal compressed a true delivered fact — the current
runtime builds servers and interfaces — into a permanent definition of the
language. The corrected wording keeps the runtime claim exact without
foreclosing another target that arrives with its own requirements and proof.
