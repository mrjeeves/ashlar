# A3 agent-reading gate — protocol

Requirement A3 asks the operational question: does a fresh agent working in
this repository, with the same `AGENTS.md` contract and reference every author
receives, understand an Ashlar snippet correctly? This corpus and protocol make
that answer repeatable and machine-scoreable. The baseline is part of the
language surface, not contamination: an agent is not expected to write Ashlar
without the file the environment always supplies.

## What the corpus is

25 files, `01`–`25`, each a pair:

- `NN-slug.ash` — a self-contained, valid Ashlar snippet (space header, any
  `use` it needs, and a minimal definition of anything it references from
  another space). A few snippets model a two- or three-file program using a
  `// file: b.ash`-style comment as a separator; this is a corpus authoring
  convention only, not Ashlar syntax — it marks "everything below this line
  is a different file" for a human or model reading the snippet.
- `NN-slug.expect.md` — a one-paragraph correct reading, followed by a
  `## Must state` list of 3–5 objective bullets: the facts a correct agent
  reading has to include. These bullets are the judge's entire rubric. They
  are written to be checked as true/false against a candidate answer, not as
  a style guide — each one names a specific, falsifiable claim about scope,
  merge order, storage, or evaluation.

The corpus is fixed. Do not edit `.ash` files to make a failing model pass;
if a snippet turns out to be genuinely ambiguous, that is itself an A3
finding to raise against the language, not the test.

## How the gate runs

1. **Fresh agent, real baseline.** For each snippet, start a fresh agent rooted
   in this repository. Do not strip or replace its project instructions:
   `AGENTS.md` is the baseline under test. Do not give it the corpus rubric,
   previous results, or any extra Ashlar explanation. Instruct it not to open
   `suites/t_a3/`; the snippet is pasted into the prompt, and the expected file
   must remain unavailable.
2. **One snippet per agent.** Paste the `.ash` file's contents verbatim and ask
   exactly:

   > State precisely what this code means/does.

   Do not add hints, name the feature being tested, or answer follow-up
   questions. One prompt and one answer per fresh agent prevents one fixture
   from teaching the next.
3. **Judge each answer against its rubric.** A judge (a separate model call or a
   human) reads the candidate's answer next to the snippet's `## Must state`
   bullets and scores each bullet independently, all-or-nothing: a bullet is
   either clearly and correctly stated (equivalent wording is fine; the fact
   must be present and correct) or it is not. Partial credit within a bullet is
   not allowed. The judge separately flags any actively wrong claim about
   merge, order, or storage semantics; such a claim fails the snippet regardless
   of how many bullets were checked off.

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
  least 20 of 25). Below that, A3 is not satisfied and the language/reference
  surface needs revisiting before the corpus is called done.

## Recording results

Each run of the gate against a model writes one file:

```
suites/t_a3/results/YYYY-MM-DD-<model>.md
```

using the date the run was performed and a short model identifier (e.g.
`2026-07-22-claude-sonnet-5.md`). That file records, at minimum:

- the model under test and the date;
- **the baseline evidence from step 1** — the agent surface and revision used,
  confirming that normal repository instructions were present and the rubric
  was not;
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

2026-07-27 — **A3 was corrected to measure the environment Ashlar authors
actually inhabit.** Once the complete reference moved into `AGENTS.md`, calling
that context contamination tested a reader who cannot exist in normal work.
The gate now starts a fresh in-repository agent for each snippet, preserves the
injected contract/reference, and withholds only the rubric and prior answers.
Runs 1–5 remain historical evidence for the earlier no-reference question; a
new run is required against the corrected requirement.
