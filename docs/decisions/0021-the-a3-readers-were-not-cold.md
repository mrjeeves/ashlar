# ADR-0021 — The A3 readers were not cold

**Status:** accepted and applied, 2026-07-25.
**Corrects the record of:** gate runs 3 and 4, ADR-0019's score claims,
ADR-0020's cold read of the four setting spellings.
**Revises:** `suites/t_a3/PROTOCOL.md` step 1 and its recording rules.

## The finding

`PROTOCOL.md` step 1 says a reader "must not have `reference/ashlar.md`,
`docs/diagnostics.md`, this repo, or any other Ashlar material in context — no
system prompt excerpting the spec, no retrieval over the repo, nothing."

Runs 3 and 4 were executed as in-repo subagents, and **every in-repo agent
receives `CLAUDE.md` — which is `@AGENTS.md` — as project instructions,
automatically, before it sees any prompt.** At the time of those runs AGENTS.md
carried a section titled "Writing Ashlar code (examples, fixtures, tests)" whose
purpose was precisely to state the syntax facts agents get wrong. It told the
reader, unprompted:

- `let` takes no shape annotation;
- `=> {` always opens a block, and how to return a map instead;
- that there is no shadowing anywhere, and which diagnostics say so;
- that `stack`/`pipe` restate their kind on every layer, and that pipe layers
  agree in parameter and return shape;
- that handlers receive `std.Event` and an input's text is `e.data.value`;
- that map shapes are written `{text: Shape}`;
- that an instance is its root element, that nested children reuse their
  instance across re-renders, and that styling goes by class name.

Those are answers to rubric bullets. The readers were not cold. Run 3's method
section says they were "provably cold" because each was "explicitly forbidden
from reading, listing, grepping, or searching any file" — true, and beside the
point: the leak was not a file the reader opened, it was a file the harness had
already pasted into its system prompt. I wrote that sentence believing it.

## Which runs are affected, exactly

Neither `CLAUDE.md` nor `AGENTS.md` existed in the repository until commit
`2a663b4` (2026-07-23). Runs 1 and 2 were performed 2026-07-22. So:

| run | date | project instructions injected | status |
|---|---|---|---|
| 1 | 2026-07-22 | none existed | valid cold read — 5/24 strict FAIL |
| 2 | 2026-07-22 | none existed | **valid cold read — 23/24 PASS** |
| 3 | 2026-07-25 | AGENTS.md incl. the syntax section | not a cold read |
| 4 | 2026-07-25 | AGENTS.md incl. the syntax section | not a cold read |

This was checked, not assumed: `git show 2a663b4^:AGENTS.md` and
`git show 2a663b4^:CLAUDE.md` both report the path does not exist in that tree.

## What survives and what does not

Contamination is directional. Extra knowledge can turn a reader's wrong answer
right; it cannot turn a right answer wrong. Therefore:

- **PASSes under contamination are not evidence.** Run 3's 23 passes and run 4's
  2 passes are withdrawn. The numbers **23/25 and 25/25 are withdrawn as gate
  results** wherever they were claimed.
- **FAILs under contamination stand**, and stand *more* strongly than a clean
  fail: the reader misread the construct while holding a cheat sheet. Run 3
  failed `owned` and `reads`/`writes` unanimously. Those findings are real, so
  **ADR-0019 stands entirely** — its respellings to `peruser` and
  `watches`/`updates` were the right call on sound evidence.
- **What is now unproven is that the new spellings read correctly.** Run 4's two
  passes were the evidence for that, and they are withdrawn. Mitigating fact,
  recorded because it bounds the doubt rather than erasing it: none of
  `peruser`, `watches`, or `updates` appeared anywhere in AGENTS.md, and neither
  did per-user scope or reactive invalidation, so the leak could not have
  supplied those specific bullets. That is an argument, not a measurement.
- **ADR-0020's cold read of `setting`/`given`/`config`/`bound` is likewise not a
  gate result.** It already said so; this ADR is the reference it was pointing
  at. The four words are absent from AGENTS.md, so the same bounded-doubt
  argument applies, and the decision to spell it `setting` rested on a name
  collision (`config` is already a part in `press` and a space in two fixtures),
  which is a fact about the corpus and unaffected.

**A3's standing evidence is therefore run 2: 23/24 PASS, clean, 2026-07-22** —
against the 24-snippet corpus as it stood that day, before `25-foreign-reactive`
existed and before ADR-0019 respelled fixtures 11 and 25. The requirement has
been met by measurement; it has not been re-measured since the language changed.
That is a weaker claim than the repo was making yesterday and a stronger one
than "unknown."

## The decision

1. **AGENTS.md must not teach the language.** The syntax section moved to
   `docs/writing-ashlar.md`, linked by path with no `@`-import, so it is
   available to any agent that needs it and injected into none.
2. **The invariant gets a test, not a promise.**
   `t_meta_agents_md_does_not_teach_the_language` asserts that AGENTS.md
   contains no fenced ```ash block, no backticked token that is an Ashlar
   reserved word (the word list is read out of `lexer.rs`, so it cannot drift),
   no `std.` reference, and no `@`-import; that the pointer to
   `docs/writing-ashlar.md` exists and resolves; and that `CLAUDE.md` imports
   nothing but AGENTS.md. One backticked keyword in AGENTS.md now fails the
   suite. This cost one wording change: hard rule 9 says "visibility closure"
   where it used to name the keyword.
3. **The protocol names the real threat model.** Step 1 now says the reader must
   not receive the repository's project instructions, calls out that in-repo
   subagents receive them automatically, and requires a positive check:
   the reader reports what project instructions it was given, verbatim, and
   that report is recorded in the results file. A leak is now visible in the
   record instead of inferred from it two days later.
4. **Runs 3 and 4 are annotated, not deleted.** Each results file gets a header
   stating it is not a valid cold read, what leaked, and which of its findings
   survive. Deleting them would destroy the failure evidence ADR-0019 rests on.
5. **A3 returns to the roadmap's open section** with the one thing that closes
   it: a full 25-fixture run under the revised protocol.

## Why the fix is a file move and not a rule about prompts

The obvious alternative — "remember to strip the syntax from the reader's
context" — is the class of fix that fails the next time. The contamination was
not an oversight in a prompt; it was structural, a consequence of putting
language documentation in the one file the harness injects everywhere. Moving
the file changes what is possible, and the test makes the regression loud. A
reminder would have changed only what was intended.

The residual leak is stated plainly rather than papered over: an in-repo reader
still receives AGENTS.md's workflow content, which names the project, says it is
a composition language, and mentions that layers and merge kinds exist. That is
strictly less than a syntax section and strictly more than nothing. A run under
the revised protocol is a *reduced-contamination* run and must be recorded as
one. A provably clean read requires a reader whose working directory is not this
repository — a fresh chat, or a session started elsewhere with the snippet
pasted in. Per §1 of the requirements that is exactly the case where an agent
says what evidence it would need instead of manufacturing a number, so: that is
what a future run 5 needs, and the roadmap says so.

## What this does not change

No language surface, no diagnostic, no code outside the meta test. Requirement
A3 is untouched — it asked the right question, and the corpus asked it correctly.
The gate caught two real design bugs even while contaminated. What failed was
the isolation of the harness around it, and the reason it took two days to
notice is that the leaking file was the one every agent is told to read first.

## Reversal (2026-07-27) — the guard is gone, and so is the in-repo run

`t_meta_agents_md_does_not_teach_the_language` is **deleted**. A1's budget was
re-read to cover one file — `AGENTS.md`, with the language reference inside it,
under 40,000 bytes total (see `docs/requirements.md`, A1 revision) — because an
agent here receives that file automatically and then goes looking for the
language, so what it holds at once is both. Once `AGENTS.md` *is* the reference,
a test banning Ashlar syntax from it contradicts itself, and a narrowed version
guarding only the workflow half would keep the letter and none of the point: the
reader receives the whole file either way.

This does not weaken A3; it removes the pretence that an in-repo run could ever
approximate a cold one. Contamination used to be partial and measurable — the
*reduced-contamination* category above. It is now total and certain, so that
category is gone with it, and `suites/t_a3/PROTOCOL.md`'s outside-the-repository
run is not merely the stronger proof but the only one.

The cost is real and is not rounded off: the cheap in-repo approximation that
produced runs 1–5 no longer exists, and the standing 24/25 is the last
measurement takeable under the old arrangement rather than one that can be
re-taken on demand. What this ADR got right survives — the leak was structural,
not an oversight in a prompt, and the fix had to change what was *possible*
rather than what was intended. Merging the files changes what is possible in the
other direction, and the honest consequence is that the gate now costs a reader
outside this repository every time it runs.
