# ADR-0019 — A3 run-3 findings: `owned` and the reactive foreign annotation

**Status:** accepted and applied, 2026-07-25.
**Evidence:** `suites/t_a3/results/2026-07-25-sonnet-run3.md`, plus the
candidate run recorded below.
**Amends:** ADR-0015 (`owned` -> `peruser`) and ADR-0014's reactive stage
(`reads`/`writes` -> `watches`/`updates`).

## What the gate found

A3 gate run 3 scored **23/25**, clearing the 80% bar, so A3 is satisfied as a
requirement. The two snippets that failed are the two constructs added since
run 2, both unanimous across a three-judge panel:

- **A3-F5** — `owned stored items` read as *encapsulation*: "state this part
  holds itself, as opposed to a prop passed in." Per-user scoping never
  appeared. The reader concluded the snippet declares one shared persisted
  list.
- **A3-F6** — `reads Row` / `writes Row` read as a *static effect annotation*:
  "declaring the side effects of opaque native code so the checker can reason
  about them." Reactivity was entirely absent — no views, no re-rendering, no
  propagation. Worst score in the corpus (2/4).

Both matter more than a missed bullet. `owned` misreads toward a security bug,
because the whole point of the modifier is that per-user isolation be the naive
reading. `reads`/`writes` misreads into a *plausible and useful-sounding* model,
which is the A4 failure mode by name: false familiarity beats unfamiliarity for
producing bugs, because nothing about the guess fails loudly.

## The candidate run, and the mistake it exposes

Candidate spellings were cold-read the same way, two independent fresh readers
each, scored only on whether the intended meaning came through:

| candidate | conveyed | what the reader took it to mean instead |
|---|---|---|
| **`peruser stored items`** | **2/2** | — |
| `each stored items` | 0/2 | per-**instance** — one copy per `Store` object |
| `owned stored items` | 0/2 | Rust-style exclusive ownership; the part's own encapsulated state |
| `personal stored items` | 0/2 | `private` — encapsulated to the part, not exposed outside |
| `mine stored items` | 0/2 | ownership/visibility qualifier; private to the part |

| candidate | conveyed | notes |
|---|---|---|
| **`watches` / `updates`** | **2/2** | "subscribes to it — a live/reactive query"; "producer/consumer reactivity link" |
| **`tracks` / `invalidates`** | **2/2** | "any cached result of `all()` must be recomputed … without manually wiring cache invalidation" |
| `observes` / `changes` | 0/2 (partial 2/2) | reactivity surfaced only as a hypothesis — "the kind of annotation a reactive runtime *would* use" |
| `reads` / `writes` | 0/2 | effect/capability annotation for the type checker; no reactivity at all |

**`personal` scoring 0/2 is the important number.** ADR-0015 justified `owned`
with a cold read in which `owned`, `personal`, and `user` all "read per-user
3/3," and rejected `private` because it landed in the OOP access-control frame.
Read inside the construct, `personal` lands in that same frame — a reader
literally answered "analogous to `private`" — and so does `owned`.

That is a methodological finding, not a vocabulary one: **ADR-0015 tested what
the word suggests in isolation; A3 is defined over constructs.** A word in a
sentence competes with the grammar around it, and `<modifier> stored items` is
a slot that readers arrive at already primed for a visibility or ownership
qualifier, because that is what that slot means in every language they know.
`peruser` survives because it cannot be parsed as visibility at all.

`each` is the instructive near-miss: it conveys the right *shape* — one copy
per something — and the wrong *something*. That is a better failure than
`owned`'s, and still a failure.

## The decision

1. **`owned` → `peruser`.** The only candidate that conveys the meaning, and
   the finding it fixes is the security-adjacent one. Cost: the reserved-word
   list, the parser, `E029`'s message and catalog row, reference §1/§4/§9.3,
   the `locker` example, T-G's fault proof, and A3 fixture `11`. `ashlar rename`
   cannot do this one — it renames names in a program, not keywords in the
   language.
2. **`reads`/`writes` → `watches`/`updates`.** Two candidates tie at 2/2;
   `watches`/`updates` is preferred on Ashlar's own house style, which favors
   short plain words over mechanism jargon (`state`, `stored`, `stack`, `pipe`,
   `append`, `deep`). `invalidates` is cache vocabulary and the longest token
   either pair would add to the reference. `watches`/`updates` also stays
   symmetric: two present-tense verbs about the same collection.
3. **Re-run A3 fixtures 11 and 25**, and record run 4. A candidate cold read
   is evidence for a choice; it is not a corpus score, so the findings stay
   open until the corpus is re-scored.

## Applied

Both renames are in. `owned` -> `peruser`, `reads`/`writes` ->
`watches`/`updates`, across the lexer's reserved words, the token set, the
parser, the AST and resolved models, the composer's storage identity, the
formatter, the evaluator's per-user scoping and its two runtime faults, `E029`'s
cause and machine fix, reference §1/§4/§9.3/§9.10, the diagnostics catalog, G4,
the `locker` and `ledger` examples, and the A3 fixtures (`11-owned` is now
`11-peruser`).

Two consequences worth stating:

- **`owned`, `reads`, and `writes` are ordinary identifiers again.** Nothing
  reserves them, so a part may declare a property called `reads` — and
  `commons` already does (`reads = put(reads, uid, ...)`), which is now a live
  proof that the retired spellings carry no special meaning.
- **The A3 fixtures were edited, and that is not corpus tampering.** The
  protocol forbids editing a snippet to make a failing reader pass. These
  edits keep the snippets *valid Ashlar* after a keyword changed; the fixture
  still asks the same question about the same construct. Re-scoring them is
  run 4's job, and until that runs the finding stays on the roadmap: this ADR
  changes the spelling, it does not claim the new spelling passes the gate.
  The candidate evidence above says it should; a candidate read is not a
  corpus score.

## The lesson worth keeping either way

**Cold-read the construct, never the word.** ADR-0015's word-level read was
run in good faith and produced a wrong answer with a clean 3/3 score, and it
took a construct-level read to see it. Any future naming decision in this repo
should test the syntax a reader will actually meet — in its slot, with its
neighbors — because the slot carries meaning the word has to fight.
