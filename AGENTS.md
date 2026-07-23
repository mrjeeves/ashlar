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
3. **The only `unsafe`** is the `dlopen` foreign boundary in `eval.rs`.
   Do not add more.
4. **No stubs** (`t_no_stubs`): no `todo!`, `unimplemented!`, or
   commented-out "coming soon" surface. A command or construct that
   doesn't fully work does not exist.
5. **Diagnostic ids are stable** (`docs/diagnostics.md`): E001–E028 +
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
   atomic apply, post-verify rollback, byte-identical reversibility
   (`move`'s stated class excepted — ADR-0009).

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
