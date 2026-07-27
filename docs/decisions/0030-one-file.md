# 0030 — One file: the reference lives in AGENTS.md

Date: 2026-07-27. Status: accepted. Supersedes the isolation half of ADR-0021
and ADR-0023.

## Context

A1 caps "the complete language reference" at 40,000 bytes, and T-A1 measured
`reference/ashlar.md` against it. That reading missed what the budget is for.
An agent arriving at this repository does not read the reference alone; it
receives `AGENTS.md` automatically (via `CLAUDE.md`) and then goes looking for
the language. What it must hold at once is **both**, and only one of them was
budgeted. The two files together were 48,704 bytes — 22% over a cap that
believed itself satisfied.

The vision's first principle is that "the surface stays small enough to hold at
once." A budget that governs half the surface does not serve it.

## Decision

**There is one file: `AGENTS.md`, at most 40,000 bytes, carrying the working
contract and the complete language reference.** `reference/` is deleted. T-A1
measures the whole file; T-A2 extracts its ```ash blocks; T-A5 measures only the
half after the `<!-- REFERENCE:BEGIN -->` marker, because A5 governs constructs
and a workflow section is not one.

Both halves paid, and which one paid what was decided by what binds it:

- **The reference could not lose a construct.** A2 says no correct program
  requires knowledge outside it, and T-A2 asserts every fixture keyword appears
  in it. So: prose tightening only. 35,152 → ~33,000, with §12 condensed and
  the illustrative Python worker replaced by the one sentence that defines it
  (the envelope was already specified above it in prose).
- **The contract could lose justification.** Its arguments live in
  `docs/decisions/` and `git log`, and the file's own last rule says a third
  prose copy is cruft. 13,711 → ~6,900. Every rule survived; the essay defending
  the hierarchy became a pointer to `docs/requirements.md` §1–2, which had been
  saying the same thing in nearly the same words.

Three real defects surfaced while trimming and are fixed in the same pass: §4
pointed at "§12" for the `rekind` refactor (it is §11), §2 pointed at "§11" for
the manifest (it is §10), and §9.12 sat before §9.11.

## Consequences

**`t_meta_agents_md_does_not_teach_the_language` is deleted.** It banned Ashlar
syntax from `AGENTS.md` because auto-injected language facts voided A3 gate runs
3 and 4. Once `AGENTS.md` *is* the reference the test contradicts itself, and
keeping a narrowed version would preserve its letter and none of its point —
the reader receives the whole file either way.

**A3 is therefore external-only.** No in-repo run can approximate a cold read
any longer; there is no longer a "reduced-contamination" category to measure,
because contamination is now total and certain. `suites/t_a3/PROTOCOL.md`
already described running from outside the working directory as the stronger
proof, and that is now the only proof. This is a real cost: the cheap in-repo
approximation that produced runs 1–5 is gone, and the standing 24/25 stands as
the last measurement made under the old arrangement rather than a result that
can be cheaply re-taken.

**Every in-repo agent now loads ~40KB of project instructions unasked.** That is
the intended trade — one file, everything needed, nothing to go looking for —
and it is the reason the budget is a hard gate rather than a guideline.

`t_meta_core_docs_exist` now asserts the merge holds in both directions: the
marker and the reference heading are present, and `reference/` does not exist.
A second copy is exactly the drift this removed.
