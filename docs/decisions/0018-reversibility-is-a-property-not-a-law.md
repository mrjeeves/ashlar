# ADR-0018 — Reversibility is a property of a refactor, not a law over all of them

**Status:** accepted, 2026-07-25
**Supersedes nothing.** Generalizes the trade ADR-0009 took for `move`.

## The requirement as it stood

> **E4.** Every refactor is atomically reversible. Forward then back yields
> byte-identical source.

Two claims wearing one number. The first — a refactor can be undone, and
undoing it restores the program — follows straight from the vision:
*"changing intent must have computable blast radius … the unread portion is
only safe when changes to it are provably contained."* The second —
reversal is byte-exact — does not appear in the vision at all. It was
chosen as the *proof mechanism* for the first: if forward-then-back
reproduces every byte, the change set was provably exactly what the radius
claimed, and that is mechanically checkable in one `assert_eq!`.

Good proof mechanism. The mistake was promoting it to a law.

## What the law cost

A refactor that must **add** a declaration to keep the program correct can
never be byte-reversible, because removing that declaration on the way back
is not always safe. `move` is the standing example: it adds the `use` lines
both sides need and never removes one, since removal can silently cut
another space's transitive closure. ADR-0009 had to carve out an exception
and state its class, and said the quiet part already:

> The alternative — refusing all moves to keep E4 universal — fails E6
> harder: relocating a part would stay a text edit.

That is the shape of the problem generally. E6 asks that the command set be
complete enough that *editing text to refactor is never the easier path*.
A universal byte-identity law puts E4 in direct conflict with E6, and
resolves it the wrong way: it makes the toolchain refuse work it can do
correctly, sending the author back to a text editor — where there is no
radius report, no atomicity, and no verification at all. Refusing a correct
refactor to protect an invariant about *bytes* trades away the very
containment the invariant was introduced to prove.

The exception list was also going to grow. Every future refactor that
normalizes anything, or that must introduce a declaration, arrives as
another carve-out. A requirement whose exception list grows with the feature
set is not describing a requirement.

## The decision

E4 now requires reversal **of the program**:

> the undone program **is** the program that preceded it — the same names
> resolving the same way, the same composition order, and a manifest
> identical except for recorded locations.

Byte-identity is retained as a **property that specific commands have**, not
a law all of them must satisfy. `rename` and `rekind` substitute names in
place and are byte-reversible; that is asserted in T-E and stays asserted.
`move` is reversible in the program sense, and byte-reversible within the
class ADR-0009 states. A future command may sit anywhere on that spectrum
provided it declares where.

This is the same move the vision itself makes in its closing line — *"state
derived at build time is what makes intent editable without fear"*. What
must be restored is the **state the build derives**, not the bytes the author
happened to type. The manifest is the program's identity; source is one
rendering of it, and `ashlar fmt` already owns the question of which
rendering is canonical. Two files that differ only in an added `use` line
denote the same program, and the toolchain is the authority on that, not a
byte comparison.

## Why this does not weaken anything real

The guarantee that carries the vision's weight is unchanged, and three
others still hold it in place: E2 (no stale reference survives, checkable by
exhaustive search), E3 (complete radius reported before applying), E5 (refuse
rather than partially apply). A refactor still cannot quietly do more than it
said. What it may now do is *legitimately add a line it reported*, without
that addition being a requirements violation.

The delivered facts are unchanged: every byte-identity assertion in T-E
still passes. What changed is that a future correct refactor no longer has to
choose between refusing and violating a requirement.

Because weakening a requirement without strengthening its test is how a bar
gets quietly lowered, T-E gains the assertion the new wording demands and the
old one never made: that a `move` **outside** the byte-identical class —
one that needed a `use` addition, so the sources genuinely differ — still
reverses to the same program, proven structurally rather than by bytes.

Writing that test immediately corrected the first draft of this decision.
"The same manifest modulo recorded locations" was the obvious wording and it
is **wrong**: reversing such a move leaves the `use` graph *wider* than it
started, because a move adds the edges both sides need and never removes one.
The manifest faithfully records that, so manifest equality fails — not
because meaning changed, but because visibility legitimately broadened.

The honest invariant is therefore a **subset**, not an equality: same parts
with the same homes, same composition order, same per-part layer order, and a
`use` closure that may only grow. That is not a loophole, because a widening
cannot silently change what an existing name resolves to — `use` only adds
candidate resolutions, and a name that gained a second candidate is an
ambiguity error (B3) which the refactor's own post-verify refuses before
writing anything. So either the program means the same thing, or the refactor
refused. The test asserts the subset relation in both directions and also
asserts the widening is real in its fixture, so the subset check is
load-bearing rather than a disguised equality.

That correction is the reason to prefer a stated property over an unexamined
law: the law sounded exact and was never true of `move`; the property is
weaker on paper and is actually checked.

## Rejected

- **Keep the law, refuse the refactors.** Fails E6, and pushes authors to
  text edits, which is strictly less safe than a reported addition.
- **Keep the law, remove `use` lines on reversal.** Byte-identity restored,
  correctness lost: removal can cut a third space's transitive closure. An
  invariant about bytes is not worth breaking name resolution for.
- **Two numbered requirements, E4a byte and E4b program.** Honest but
  useless: nothing would ever be allowed to satisfy only E4b, so the law
  would still bind through the back door.
- **Reverse via a stored journal** (record the inverse edit set at apply
  time, replay to undo). Byte-exact undo for every command, including ones
  that add declarations — but it makes undo depend on a build artifact that
  can be stale, deleted, or from a different program version, and "the build
  is state, the code is intent" cuts against a *source* edit that is only
  reversible while a side file survives. Reversal must be computable from
  the program, as every other refactor fact is.
