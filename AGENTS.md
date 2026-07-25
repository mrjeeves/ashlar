# AGENTS.md — the working contract for agents in this repository

You are working on **Ashlar**, an agent-authored composition language.
This file is the load-bearing entry point for AI agents: read it before
touching anything. Humans start at `README.md`; the two must never
disagree, and when they do, fixing the disagreement is part of your task.

## The one rule that orders all others

```
VISION          docs/vision.md. Fixed. If the vision is wrong, stop and say so.
REQUIREMENTS    docs/requirements.md. Revised only when it fails the vision.
TESTS           The current best encoding of the requirements. Revised freely.
CODE            Whatever makes the tests pass.
```

Lower layers yield to higher ones — always. "The test is inconvenient"
is never a reason to change a test; "the test mis-encodes the
requirement" is the only one.

**The hierarchy is a grant of authority, not only a tie-breaker.** Exactly
one layer is user-controlled: the vision. Everything below it — the
requirements, the reference, the tests, the corpora, the ADRs, the code —
is yours to change without asking, *provided the change serves the layer
above it and lands with the evidence and tests that show it does.* That is
the point of writing the hierarchy down: an agent that stops to request
permission for a requirements revision has failed the same way an agent
that quietly weakens a test has failed. Both substitute someone's comfort
for the argument from the layer above.

So: if a gate you can run says a construct is wrong, fix the construct.
If a requirement fails the vision, revise the requirement and record why.
Do not mark a decision "proposed" and wait, when the evidence to decide it
is already in hand — write the ADR as accepted and apply it.

Stop and say so in exactly two cases:

1. **The vision itself looks wrong.** That is the user's to change; you
   may argue for it, never edit it.
2. **You cannot get the evidence.** A cold-read gate needs a fresh reader,
   a benchmark needs a machine — if the deciding evidence is out of reach,
   say what you would need rather than guessing and calling it a decision.

Neither case covers "this change is large" or "this reverses an earlier
ADR." Large and reversing are normal. An earlier ADR is a record of a
decision made on the evidence then available; new evidence outranks it,
and superseding one is ordinary work.

## What outranks what, concretely

- **`reference/ashlar.md` is the language contract.** Every sentence in
  it must be true of the binary, and every ```ash block in it must
  compile with zero diagnostics (T-A2 enforces this). If you change
  behavior, change the reference in the same commit — and vice versa.
  The reference is budgeted: **≤40,000 bytes total** (T-A1), no single
  construct over 20% of used bytes (T-A5). Spend words like money.
- **This file governs the repo's workflow.** The reference governs the
  language. Don't put language rules here or workflow rules there.

## Hard rules (each has a test with teeth)

1. **The words `meld` and `pattern` are banned from the language and its
   docs** — killed during naming, never to return (T-B scans for them).
2. **Zero dependencies** (G1, `t_meta_g1_zero_dependencies`): the
   workspace has no external crates. JSON, SHA-1, HTTP, WebSockets,
   PBKDF2 — all hand-rolled in-tree. Do not add a crate; write the code.
3. **The only `unsafe`** is the `dlopen`/`dlsym` pair in `foreign.rs`,
   the module that owns the whole §9.10 boundary. Do not add more — the
   `worker` and `http` transports beside it are plain safe Rust, and a new
   capability should reach for one of those.
4. **No stubs** (`t_no_stubs`): no `todo!`, `unimplemented!`, or
   commented-out "coming soon" surface. A command or construct that
   doesn't fully work does not exist.
5. **Diagnostic ids are stable** (`docs/diagnostics.md`): E001–E029 +
   W001. New checks reuse an existing id with a new cause when they
   enforce the same requirement; a genuinely new id is appended, never
   renumbered, and its catalog row lands in the same commit.
6. **Diagnostics are corrections.** Every error states its cause in one
   sentence and the correction specifically enough to apply without
   judgment; machine edits must leave the program strictly better (D2),
   and the corpus mean rounds-to-clean stays at 1.00 (T-D5 gates ≤1.5).
7. **No false positives in the checker.** `Unknown` absorbs anything the
   checker cannot prove; a wrong error would poison trust in the
   corrections instantly. When in doubt, stay silent and note the gap.
8. **Examples are corpus** (`t_examples`): every project under
   `examples/` compiles clean, is canonically formatted, and is DRIVEN
   at runtime over real HTTP/WebSockets. A broken example is a failing
   test, not a discovery. New feature → consider showing it in an
   example; new example → it gets a runtime test.
9. **Refactors never partially apply** (E-series): blast radius first,
   atomic apply, post-verify rollback, and reversal to the same PROGRAM
   — same parts and homes, same composition order, and a `use` closure
   that may only have WIDENED (not the same manifest: `move` never
   removes a `use`, so reversal leaves visibility broader, and a widening
   that changed a resolution would be the B3 error post-verify refuses).
   Byte-identical reversal is a property specific commands have
   (`rename`, `rekind`, and `move` within ADR-0009's class), not a law
   over all of them: a refactor may add a declaration it reported rather
   than refuse correct work (ADR-0018). Do not weaken a byte-identity
   assertion that currently passes — the requirement got weaker, the
   delivered facts did not.

## The suite is the definition of done

```
cargo test                 # all 17 binaries; must be green in debug
cargo test --release       # F1 latency gate is release-only (<100ms hard)
cargo build --tests        # zero warnings, always
```

Suite map: T-A1/A2/A5 (reference gates), T-A3 (cold-read gate — run via
the protocol in `suites/t_a3/PROTOCOL.md`, not in CI), T-A4 (38
loud-failure fixtures), T-B (banned words, name hygiene), T-D/T-D5
(fix round-trips), T-E (refactor proofs), T-F/T-F1 (manifest + latency),
T-G (runtime conformance), T-META (docs/coverage/no-deps),
t_examples (the showcase, both depths). Every new behavior lands with
the test that would catch its regression — no exceptions, that is what
"done" means here.

## Writing Ashlar code (examples, fixtures, tests)

Read `reference/ashlar.md` first — it is short on purpose. The traps
that catch agents who guess instead:

- `let` takes no shape annotation; locals are inferred.
- `=> {` always opens a BLOCK; to return a map literal, write
  `=> { return { k: v } }`.
- No shadowing anywhere: a local or parameter may not reuse any visible
  name (parts, props, std) — and part names like `login`, `signup`,
  `count` collide with builtins or case-fold against other names (E002/
  E003 will tell you).
- Chain properties (`stack`/`pipe`) must restate their kind on every
  layer; pipe layers must agree in parameter AND return shape.
- Event handlers get `std.Event`; the input's text is `e.data.value`.
- Map shapes are written `{text: Shape}`; computed keys reach data only.
- Views: an instance IS its root element and nested `el(Part)` children
  reuse their instance across re-renders (`start` once, `stop` on
  removal) — so nest freely and lean on the lifecycle. Style by `class`
  name bound to the root's declared `style = "sheet"`
  (`assets/sheet.css`); a `style="..."` attribute is the wrong tool.

## Sync duties — what must move together

| you changed | you must also touch |
|---|---|
| language behavior | `reference/ashlar.md` + a test + (if user-visible failure) `docs/diagnostics.md` |
| a diagnostic's cause/fix | its `docs/diagnostics.md` row |
| a design trade | a new `docs/decisions/NNNN-*.md` ADR, never edits to old ones |
| delivered/new planned work | `docs/roadmap.md` (an empty ledger is a claim — keep it honest) |
| anything shown in `README.md` | keep README, AGENTS.md, and reality agreeing |
| the reference | re-run the gates (T-A1/A2/A5) and eyeball the byte budget |

## Operating discipline

- Work on a branch; every merged increment leaves the suite green and
  the docs true. Never commit runtime artifacts (`.ashlar-state.json`,
  `ashlar.manifest` — gitignored).
- Contract files (`tokens.rs`, `ast.rs`, `diag.rs`, `resolved.rs`,
  `lib.rs`) change rarely and deliberately — they are the interfaces
  between pipeline stages.
- When a bug is found, the fix lands WITH the regression test that
  would have caught it, in the same commit.
- Big claims get adversarial verification: this repo's practice is to
  fan out independent reviewers over a finished increment and refute
  every finding against the built binary before believing it.
- The honest sentence beats the impressive one — in diagnostics, docs,
  commit messages, and this file.
