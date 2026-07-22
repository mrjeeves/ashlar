# ADR-0006: JSON Lines diagnostics, machine-readable by default

Date: 2026-07-22

Status: accepted

## Context

D4 requires diagnostics to be structured and machine-readable first, human-
rendered second — explicitly the inverse of how every existing compiler
defaults (prose first, machine parsing bolted on afterward via regexes
against unstable text). D1 requires every diagnostic to carry a location, a
one-sentence cause, and a judgment-free correction. D2 requires that where a
diagnostic offers a machine-applicable fix, applying it actually resolves
the error without introducing a new one — which only means something if the
fix is represented as data (edits with locations and replacement text), not
as an instruction to a person. Requirements §11 leaves the wire format's
encoding unspecified — D4 specifies the ordering of concerns, not the
bytes on the wire.

## Decision

The wire format is **JSON Lines**: `ashlar check` writes one JSON object
per diagnostic, one per line (reference §8). This is the default and only
form the compiler itself reasons about; `--human` is an explicit,
opt-in rendering of the same diagnostics as prose, produced by formatting
the structured form rather than the reverse. This is the D4 inversion made
literal: every other compiler's human-readable text is primary and
machine-readable output (if it exists at all) is derived from it; here the
structured form is primary and the prose is derived.

Each diagnostic carries a stable `id` (docs/diagnostics.md; ids never
change meaning across releases, retired ids are never reused) and a `req`
field naming the specific requirement it enforces, so the connection
between "the compiler said X" and "the requirement that made X an error"
is explicit and greppable rather than living only in comments or in
someone's memory.

The fix contract is: a `fix.edits` array is attached **only** when applying
those edits resolves the diagnostic they're attached to and introduces no
new diagnostic — this is D2 as a wire-level rule, not just a design
intention. When a correction genuinely requires judgment (for example, E003
— two names differing only by case or separator — where renaming one of
them is a decision only the author can make), the `fix.note` carries the
instruction in prose and `edits` is empty, per D1's requirement that the
correction still be *specific* even when it can't be *mechanical*.
`ashlar fix` applies edit-bearing fixes mechanically, and only ever those.

## Consequences

- Tooling (editors, agents, CI) consumes `ashlar check`'s output without
  parsing prose or maintaining regexes against a human-facing format that
  can drift release to release — the JSONL shape is the contract, and
  `--human` can be restyled freely without breaking anything that reads
  the real output.
- D2 becomes testable directly: T-D can assert, for every diagnostic that
  carries `edits`, that applying them produces source that compiles past
  that specific error with no new diagnostic introduced — the assertion is
  about data, not about parsing a human sentence.
- Stability of `id`s is now a real commitment, not just a convention: an
  id retired for one meaning can never be reused for another, which is why
  docs/diagnostics.md exists as a permanent catalog rather than being
  regenerated from source comments.
- Diagnostics that cannot offer a safe mechanical fix (E003 being the
  clearest example) are documented as such rather than forced into a fake
  `edits` array — the format makes "this needs a human's judgment" a
  representable, honest state rather than something papered over.
