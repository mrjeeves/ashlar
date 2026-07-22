# 0008 — Resolving the first cold-read gate's surface findings (F1–F4)

Date: 2026-07-22. Status: accepted.

## Context

The first T-A3 run (`suites/t_a3/results/2026-07-22-sonnet.md`) failed the
strict bar and surfaced four findings. Each needs an explicit decision:
change the surface, or accept the reading risk and name the guardrail.
Requirements §2 governs: a failed guessability test is a design bug unless
the requirement itself is misapplied — and the results file already
separated genuine surface bugs (F1, F2) from prior-import hazards the
snippet could never decide (F3, F4).

## F1 — restated `stack`/`pipe` read as override: KEEP SYNTAX, name the guardrail

A derived layer's `handle pipe = ...` was read as replacing the base
handler. The decisive observation: this misread is only dangerous when
*writing* if the writer can act on it — and the writer cannot. An agent
intending override writes the kindless form `handle = ...` (replace is the
kindless default it believes in), and that is **E005, a compile error with
a correction**, because the property's identity fixes `pipe` and every
touching layer must restate it. The override-intender is caught at the
first compile; the reader's wrong model survives only until any diagnostic
or the reference enters context. A4 is satisfied by the existing identity
rule; adding surface (a participation marker, a second keyword) would buy
little and cost A1 budget plus C-series simplicity.

Consequences: E004/E005 diagnostic notes now teach the chain semantics in
one sentence, so the very first contact with the rule states the model
("every layer runs; a layer cannot replace a `pipe`/`stack` property").
A T-A4 fixture (`29-pipe-override`) pins the guardrail: a derived layer
omitting the kind on an inherited `pipe` property must produce E005.

## F2 — `{Shape}` map syntax read as a set: CHANGE THE SYNTAX to `{text: Shape}`

`{chat.data.Message}` was read as a set — braces-around-element is the
set prior, and nothing on the page pushed back. The fix is the one the
language already believes in: **make the shape look like the values it
describes.** Map literals are written `{ key: value }`; map shapes are now
written `{text: Shape}`. The key shape is always literally `text` (map
keys are text, a fixed rule — B-series simplicity), so this costs five
characters of ceremony and buys an unmistakable map reading: a colon in
braces is a map in every mainstream prior, and no set notation on earth
contains one.

The old form fails loudly with a correction (A4 + D2): `{Shape}` is a
compile error whose machine fix inserts `text: `; `{number: Shape}` (or
any non-`text` key) is a compile error whose machine fix rewrites the key
to `text`.

Consequences: reference §5 and every example updated; parser enforces the
key; t_a3 snippets and rubrics updated; the gate re-runs against the new
surface (dated results file). `ast::Shape::Map` is unchanged — the key is
not stored because it cannot vary.

## F3 — index-yields-optional invisible: ACCEPT, reference-carried

`m["top"]` inferring `number?` has no possible surface marker that is not
worse than the rule (mandatory `?` on every index would be noise the
checker already enforces). The misread fails safe: any use of the value
as a plain number is a shape error at compile time once the checker
(roadmap 1) lands, with a correction naming `??`/`!`. Accepted as
reference-carried; the checker is the guardrail and is now the top
roadmap item partly for this reason.

## F4 — determinism guarantees contradict priors: ACCEPT, reference-carried

Key-ordered map iteration, build-checked `every` durations, and
runtime-checked foreign shapes are commitments the snippet cannot exhibit
and the priors contradict. They stay reference-carried; each is stated in
one prominent sentence in the reference section that owns it. No surface
change would exhibit a guarantee; only behavior and documentation can.

## The bar this was judged against

Every decision above either changes the surface so the wrong guess cannot
be formed (F2), or names a compile-time guardrail that converts the wrong
guess into a corrected error at first contact (F1, F3), or concedes the
fact is unknowable without the reference and keeps it prominent there
(F4). Nothing is left in the fourth quadrant — wrong guess, silent
consequence — which is the only quadrant A4 forbids.
