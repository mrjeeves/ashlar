# ADR-0001: The language is named Ashlar

Date: 2026-07-22

Status: accepted

## Context

The requirements (docs/requirements.md §0, §11) deliberately leave the
language's name undecided, on the grounds that Principle 2 ("names matter
more than anything") applies to the language's own name too, and a name
adopted under time pressure is exactly the kind of decision the vision warns
against making carelessly.

A name for a language whose primary author is a machine and primary reader
is a human has to satisfy two things at once: it must be available (not
already the name of a live tool, framework, or runtime a reader would
confuse it with), and it must not make a claim about the language's
semantics that the language doesn't keep — per A4, a name is itself
guessable surface, and a wrong guess from the name is the same bug class as
a wrong guess from syntax.

## Decision

The language is **Ashlar**. Source files use the extension `.ash`; the
binary is `ashlar`.

An ashlar is a stone block cut so precisely that it can be laid without
mortar — the fit between blocks *is* the joint. That is requirement B1 as an
image: names are the only binding mechanism, there is no path, position, or
declaration order gluing things together, just names fitting names. Courses
of ashlar are laid one on top of another in order, and each course sits on
the one below it — that is the default merge kind (§6/C4 of the
requirements, §4 of the reference): later definition wins, later layer sits
on the earlier one, exactly like a course sitting on the course beneath it.
An ashlar wall is also legible by construction — every unit is visible to
the eye, none is hidden inside another — which matches the requirements'
non-goal that the primary reader is a human reviewing the machine's work.

The name also fails safely under A4: a reader unfamiliar with the word
"ashlar" gets "I don't know that word," not a false analogy to some other
system. That is the load-bearing property, and it is why alternatives that
trade on a *false* analogy were rejected on the merits rather than on
availability alone (see below).

Alternatives considered and rejected:

- **Weld** — taken twice already: a Stanford data-analytics runtime and an
  unrelated general-purpose language. Collision risk too high for a
  guessability-gated name.
- **Weft** — taken three times, including a 2026 AI-systems language.
  Same problem, worse.
- **Sinter** — the name of an existing statically-typed, garbage-collected
  language. Direct collision.
- **Cairn** — the name of an existing agent-coding tool. Direct collision
  in exactly this problem space, which is the worst kind of collision to
  carry.
- **Tenon** — the name of two existing software companies.
- The whole weaving metaphor (Loom, Weave, Weft, Warp) is exhausted — every
  obvious member of the family is already claimed by something.
- **Lattice** — rejected on the merits, not availability. A lattice join in
  the mathematical sense is commutative. Ashlar's merge is emphatically
  not commutative — order matters, later wins, `reverse` exists precisely
  because direction matters. A reader who knows the math would guess wrong
  from the name itself, which is the A4 failure mode expressed as a name
  rather than as syntax. A name that lies to the reader who knows the most
  is worse than a name that says nothing.

Additionally, `meld` and `pattern` are excluded from Ashlar's vocabulary
entirely, at every level (keywords, builtin names, documentation prose used
as a term of art). Both already carry specific, different meanings in the
sibling system Doh; reusing them in Ashlar would make both vocabularies
harder to read for anyone who has to hold both in mind.

## Consequences

- The reference, CLI, manifest header (`ashlar.manifest`), file extension
  (`.ash`), and diagnostics are the only places the language's own name
  appears in a program's lifecycle — it does not appear in source files
  themselves (no source declares what language it's written in; the
  extension carries that). This is what makes ADR-0002's claim that the
  unit keyword, not the language name, is the most-repeated name in the
  system true rather than aspirational.
- `ashlar fmt`, `ashlar check`, `ashlar vendor`, and every other subcommand
  read as verbs applied to material, which keeps the CLI's own vocabulary
  consistent with the stone metaphor without requiring the metaphor to be
  explained anywhere load-bearing (A1's budget is not spent justifying the
  name).
- Because `meld` and `pattern` are permanently off the table for Ashlar,
  any future feature that might have reached for either word (e.g. a merge
  operation, a structural-matching construct) must find a different name
  before it can be proposed — this is a standing constraint on future
  surface, not just a historical note.
