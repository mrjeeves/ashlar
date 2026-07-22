# 0009 — Refactor completeness: field renames, spaces, move, stored data

Date: 2026-07-22. Status: accepted.

## Context

Increment 9 empties the roadmap's last residual list: data-shape field
rename, space rename, `move`, `stored`-data migration, the reference §11
commands (`radius`, `vendor`) that existed in the table but not the
binary, and the D3 checker inventory (stack/pipe cross-layer agreement,
route capture rules, recursion inference). Each item forced a decision
worth recording.

## Field renames ride the checker's field-site index

The old refusal ("literals constructing the shape are not yet tracked")
is closed by tracking them where they are already proven: **the checker**.
Every map literal checked against a data shape records its keys' spans;
every `el(Part, {...})` key and every field access on a value whose shape
the checker knows records likewise. The rename planner consumes that
index. The completeness argument is structural, not heuristic: a plain
map never `fits` a data shape, so in a diagnostic-clean project every
literal that constructs one passes through `check_against` — there is no
unchecked construction channel. `data`-mediated key access (`json(...)`
walks) is deliberately outside the shape system and outside the radius:
those keys are runtime data, not program names (§12's line). Sites the
checker cannot pin to one line (multi-line chains) refuse with the
`ashlar fmt` correction — E5's shape, unchanged.

Two checker gaps found en route were fixed because the index rides on
checked positions: `put`'s value argument was inferred-then-fitted (a map
literal against `{text: Shape}` would have false-positived) and now
checks against the element shape; `el`'s field map was entirely
unchecked and now checks keys and value shapes against the part.

## Space rename is pure prefix substitution

Headers, `use` lines, dotted layer declarations, and full-name references
rewrite by column arithmetic; bare references never change because the
rename preserves the use graph edge-for-edge. Reference resolution
mirrors the resolver's longest-prefix rule, so `old.b.c` resolving into a
space named `old.b` is left alone. Forward-then-back is byte-identical,
same as part renames. A name that is both a part and a space refuses as
ambiguous rather than guessing.

## `move` adds `use` lines, never removes them — and states its E4 class

`move` excises the home block (with its preceding blank line), appends it
canonically (end of the target space's first file), rewrites full-name
references, and adds the `use` lines both sides need: the moved body's
dependencies for the target space, `use <target>` for every space that
references the part. It never removes a `use` — removal can silently cut
other spaces' transitive closures, and stale breadth is harmless where
silent breakage is not.

The E4 trade, stated plainly: forward-then-back is byte-identical **when
the part sits at the canonical position and neither direction needs a
`use` addition** (T-E pins exactly this class). Outside it, the result is
semantically identical and post-verified, but added `use` lines remain
and the block returns to the canonical position, not its original one.
The alternative — refusing all moves to keep E4 universal — fails E6
harder: relocating a part would stay a text edit. Every added line
appears in the radius report, so nothing about the class is silent.

## Stored keys migrate with their names

`.ashlar-state.json` keys by full dotted name, so ADR-0007's note stood:
a rename orphaned rows. Plans now carry the key migrations they imply
(part-level prefixes and exact prop keys); the CLI applies them after
sources verify, atomically via temp-file rename. A running server is out
of scope: its in-memory state re-flushes on the next change, so migrate
with the server stopped — the radius report prints the migration either
way.

## D3 inventory closes as E006/E021 causes, not new ids

Pipe layers must agree in parameter and return shape (§4's sentence,
now enforced); stack `return { ... }` literals must name storage props
with fitting values; route captures must be legal names bound once.
All are causes under the existing ids (E006 for shape agreement, E021
for route rules) — the catalog's ids stay stable, and D-series wording
("stable ids, growing causes") is honored. Recursion inference refines
`-> ?` returns from concrete branches (bounded fixpoint, diagnostics
discarded during speculation), so callers of `fact(n)` check instead of
absorbing into `Unknown`. A `stack` prop's call still returns the part,
so stack props are excluded from refinement.

## `vendor` refuses collisions before copying, rolls back after

`ashlar vendor <source>` copies a tree's `.ash` files under
`vendor/<name>/`. A tree redeclaring an existing space refuses before
anything is written (extending someone's space silently is a composition
change, not a dependency add). If the combined project does not check,
the copy is removed entirely — atomic or not at all, same doctrine as
the refactors.
