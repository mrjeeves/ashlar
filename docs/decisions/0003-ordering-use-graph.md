# ADR-0003: Composition order is the use graph

Date: 2026-07-22

Status: accepted

## Context

C2 requires composition order to be deterministic and computable from
declarations alone, independent of file layout. C3 requires that any
arbitrary tie-break be flagged with a warning naming both sides and the
declaration that would resolve it. B3 requires every name to resolve to
exactly one definition, with no shadowing. Something has to answer, for
every part with more than one layer, "which layer sits on which" — and
that answer has to come from something the author already declares for an
unrelated reason (B7's transitive `use`), not from a second ordering
mechanism invented just for this purpose, because a second mechanism is a
second thing to learn and a second way to be wrong.

## Decision

Composition order **is** the use graph. Spaces are the unit of scope *and*
the unit of order at once: if space B uses space A, directly or
transitively, B's layer of any part sits on A's layer of that part. There
is no separate ordering declaration.

Because `use` is transitive (B7 — using a space brings in everything that
space uses, all the way down), any part you are able to name at all is a
part whose space you have a `use` path to, and a `use` path is exactly an
ordering relationship. C3's tie-break warning can therefore only ever fire
between two spaces that are *true siblings* — neither reachable from the
other through any `use` chain — because anything else already has a
declared order by construction. This is why C3 is a narrow, occasional
warning (W001) rather than a routine one: most pairs of layers are already
ordered by the graph a program needed to write anyway.

Each space may declare at most one layer of a given part; a second layer
of the same part in the same space is redundant with the first (nothing
would distinguish which of the two same-space declarations came "before"
the other) and is rejected as a compile error (E014), with the fix being
to merge the two blocks into one.

A dotted part declaration (`part chat.data.Message { ... }` from a space
other than `chat.data`) must match the full name of a part that is already
visible through the use graph. A dotted name that matches nothing is a
compile error naming the nearest match (E001) rather than silently
introducing a new, differently-spelled part — this is the A4 guarantee
applied specifically to layering: a typo in a dotted part name must be loud,
never a quiet fork of the part.

Where two spaces genuinely layer the same part and neither uses the other,
ties are broken lexicographically by space name, and the compiler emits
W001 naming both layers and the specific `use` declaration that would
order them deliberately. This keeps the tie-break itself deterministic
(satisfying C2 even in the unordered case) while still surfacing the
warning C3 requires, because a lexicographic tie-break is still an
*arbitrary* one from the author's point of view — it happens to be
computable, not to be intended.

No shadowing exists anywhere in the language (B3, reference §2 and §7): a
bare name that could resolve to more than one visible part is ambiguous
even if one of the candidates lives in the current space. Ambiguity is
always a loud, mechanical error (E002) with a mechanical fix — qualify the
reference with its full dotted name — never a "closest scope wins" rule
that would make the answer depend on where you're standing.

## Consequences

- Authors get exactly one relationship to declare (`use`) and it does
  double duty for both requirement B7 (name visibility) and requirement C2
  (layer order). There is nothing named "order" in the reference at all —
  §3 of the reference describes composition order entirely in terms of
  `use`.
- The tie-break warning (W001) is rare by construction, which keeps it
  meaningful when it does fire — it never fires for a pair of spaces that
  have any `use` relationship between them, direct or transitive.
- The choice forecloses ever adding an explicit "order:" or priority
  annotation to parts or properties without deprecating this ADR — doing
  so would create two ordering mechanisms that could disagree, which is
  exactly the failure mode C2 exists to prevent.
