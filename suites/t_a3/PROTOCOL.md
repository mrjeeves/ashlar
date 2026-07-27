# A3 guessability gate — protocol

Requirement A3 asks a narrow question: if you show a competent reader one
Ashlar snippet, with no reference material and no repo access, does it read
the way it means? This corpus and this protocol are how that question gets
a repeatable, machine-scoreable answer. A failure here is not a bug in a
model's knowledge — it is evidence that a piece of Ashlar syntax invites a
wrong mental model, which is a design bug in the syntax.

## What the corpus is

25 files, `01`–`25`, each a pair:

- `NN-slug.ash` — a self-contained, valid Ashlar snippet (space header, any
  `use` it needs, and a minimal definition of anything it references from
  another space). A few snippets model a two- or three-file program using a
  `// file: b.ash`-style comment as a separator; this is a corpus authoring
  convention only, not Ashlar syntax — it marks "everything below this line
  is a different file" for a human or model reading the snippet.
- `NN-slug.expect.md` — a one-paragraph correct reading, followed by a
  `## Must state` list of 3–5 objective bullets: the facts a correct cold
  reading has to include. These bullets are the judge's entire rubric. They
  are written to be checked as true/false against a candidate answer, not as
  a style guide — each one names a specific, falsifiable claim about scope,
  merge order, storage, or evaluation.

The corpus is fixed. Do not edit `.ash` files to make a failing model pass;
if a snippet turns out to be genuinely ambiguous, that is itself an A3
finding to raise against the language, not the test.

## How the gate runs

1. **Fresh model, no context.** Start a new conversation with the model
   under test. It must not have `reference/ashlar.md`, `docs/diagnostics.md`,
   this repo, or any other Ashlar material in context — no system prompt
   excerpting the spec, no retrieval over the repo, nothing. The only prior
   knowledge it may draw on is whatever it already knows unprompted.

   **This includes the repository's project instructions, which are injected
   automatically.** An in-repo agent receives `CLAUDE.md` — and everything it
   `@`-imports — before it sees any prompt. Forbidding the reader to open
   files does not help: nothing was opened. This is how runs 3 and 4 were
   invalidated (ADR-0021), and why `AGENTS.md` no longer states any language
   fact, a rule with a test
   (`t_meta_agents_md_does_not_teach_the_language`).

   Therefore the isolation is **verified, not assumed**: before or after the
   read, ask each reader to report verbatim the project instructions it was
   given, and record that report in the results file (once per run is enough
   if every reader is spawned identically). If it contains a statement of
   Ashlar syntax or semantics, the run is void — fix the leak and re-run.
   Knowing the project's name and that it is a composition language is an
   unavoidable consequence of running inside the repo; a run so isolated is
   **reduced-contamination** and must be labelled that way, not "cold".

   **To run a provably cold one**, the reader's working directory must not
   be this repository — no in-repo agent can produce it, because the
   project instructions are injected before any prompt exists to forbid
   them. Copy the 25 snippets out (they are self-contained text), open a
   chat or a session rooted anywhere else, and paste them one per clean
   turn with the question below. Record the run with `cold` in its filename
   and the isolation report showing project instructions that mention no
   project. Every scored run to date is reduced-contamination, and the
   scores have been reproducible across three of them — a cold run is a
   stronger proof of the same claim, not a different claim.
2. **One snippet at a time.** For each of the 25 `.ash` files, in a clean
   turn (no memory of previous snippets in the run — treat each as an
   independent cold read), paste the file's contents verbatim and ask
   exactly:

   > State precisely what this code means/does.

   Do not add hints, do not name the language feature being tested, do not
   answer follow-up questions about it. One prompt, one answer, per
   snippet.
3. **Judge each answer against its rubric.** A judge (a separate model call
   or a human) reads the candidate's answer next to the snippet's
   `## Must state` bullets and scores each bullet independently,
   all-or-nothing: a bullet is either clearly and correctly stated
   (equivalent wording is fine; the fact must be present and correct) or it
   is not. Partial credit within a bullet is not allowed — half-stating a
   fact scores it as not stated. The judge also flags, separately, whether
   the answer contains any **actively wrong claim about merge, order, or
   storage semantics** (e.g. claiming `append` replaces instead of
   concatenating, claiming layers run derived-to-base when the snippet has
   no `reverse`, claiming `state` persists across restarts, claiming `use`
   order is alphabetical) — such a claim fails the snippet regardless of
   how many bullets were separately checked off.

## Pass/fail definition

- **A snippet passes** when both hold:
  - at least 75% of its `## Must state` bullets are scored correct (round
    down the bullet count needed: 4 bullets need 3 correct, 5 bullets need
    4 correct, 3 bullets need all 3 correct);
  - AND the answer contains no actively wrong claim about merge, order, or
    storage semantics, even if that claim isn't one of the listed bullets.
    A snippet with 100% of its bullets checked off but one confidently wrong
    claim elsewhere in the answer still fails.
- **The corpus passes** when at least 80% of its 25 snippets pass (i.e. at
  least 20 of 25). Below that, A3 is not satisfied and the syntax needs
  revisiting before the language, not the corpus, is called done.

## Recording results

Each run of the gate against a model writes one file:

```
suites/t_a3/results/YYYY-MM-DD-<model>.md
```

using the date the run was performed and a short model identifier (e.g.
`2026-07-22-claude-sonnet-5.md`). That file records, at minimum:

- the model under test and the date;
- **the isolation evidence from step 1** — what project instructions the
  readers received, and whether the run is labelled cold or
  reduced-contamination;
- for each of the 25 snippets: pass/fail, which bullets were checked correct
  (by number), and whether an actively-wrong-claim flag was raised;
- the overall corpus score (`<passing>/25`) and pass/fail against the 80%
  bar;
- verbatim or lightly-trimmed candidate answers for any snippet that
  failed, so a syntax fix can be judged against the actual wrong reading.

Do not overwrite a previous run's results file; each run gets its own dated
file, so regressions and improvements across model versions or language
revisions are visible side by side.

## Revisions

2026-07-22 — First run (`results/2026-07-22-sonnet.md`, 5/24 strict FAIL)
showed the rubrics mixing two populations: meaning-of-what-is-shown, which
a no-reference cold read can measure, and system behavior the snippet does
not exhibit (compile-error obligations, runtime lifecycle facts, protocol
transparency), which it definitionally cannot. Per requirements §1 — tests
are revised freely against the requirement, and A3 asks whether a reader
"states the meaning" of the construct shown — future rubric bullets must be
decidable from the snippet text plus universal programming knowledge.
Bullets about unexhibited behavior move to a separate reference-in-context
comprehension suite. The strict 2026-07-22 result stands as recorded;
re-baseline against the recalibrated rubrics after the F1/F2 surface
findings in that results file receive a design decision.

2026-07-25 — Step 1 gained the project-instructions clause and the positive
isolation check above, and "Recording results" gained the isolation-evidence
line, after runs 3 and 4 were found to have read with `AGENTS.md`'s syntax
section in their system prompts (ADR-0021). The corpus and rubrics are
unchanged; only the isolation requirement and how it is evidenced changed.
Runs 3 and 4 keep their files, annotated as void; their FAILs remain valid
findings, because a leak can only raise a score. A3's standing evidence is
run 2 (23/24, clean, against that day's 24-snippet corpus).

2026-07-25 (later) — **Run 5** exercised the revised step 1 and it worked. The
probes found that `AGENTS.md` still states language facts despite saying it does
not (refactor command names, transport names, two banned words), so the honest
rule is narrower than "states no language fact": **the reader must not be handed
a fact a rubric asks that reader to produce.** That is now the property
`t_meta_agents_md_does_not_teach_the_language` asserts, by intersecting
AGENTS.md's inline-code spans with every one in every `## Must state` list. Run 5
also produced the direct evidence for ADR-0021's directional argument:
`08-handle-pipe`, the one fixture whose fact the removed section stated, is the
one fixture that flipped to PASS in run 3 and back to FAIL here. A3's standing
result is now **run 5: 24/25, reduced-contamination**, and `11-peruser` /
`25-foreign-reactive` scored 4/4 clean, closing A3-F5 on measurement.
