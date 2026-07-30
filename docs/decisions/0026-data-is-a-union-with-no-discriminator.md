# ADR-0026: `data` is a union with no discriminator

Date: 2026-07-26

Status: accepted; the discriminator landed 2026-07-27, the status half is
withdrawn (see Resolution)

## Context

Everything that arrives from outside a program is `data`: a request body,
a foreign return, a parsed `json(t)`. §5 defines it as a union — "any of:
text, number, bool, none, list of data, map of data" — and §9.2 hands
`req.data` to every handler.

`examples/quarry` was written as a public status board, so its ingest
route is open to whatever the internet posts at it. Driving it with
hostile bodies produced three distinct outcomes, and only one of them is
good:

| body | what happened |
|---|---|
| `{"line":"crate","load":65}` | accepted |
| `{"line":"crate","load":"banana"}` | **accepted as a reading of 0** |
| `{"line":"crate"}` (no load) | **accepted as a reading of 0** |
| `not json at all` | fault → **500** |
| `[1,2,3]`, `42`, `"hello"` | fault → **500**, message `internal: cannot index a list with text` |

**The first failure is ergonomic, and it is the language's fault.** The
idiom that type-checks in the fewest characters is
`number(text(req.data.load)) ?? 0`, and it launders bad input into a
plausible value: a reading of zero that nothing downstream can distinguish
from a real one. `??` is the operator the checker pushes you toward at a
boundary — an optional must be dealt with, and the shortest way to deal
with it is to invent a default. Rejecting instead takes a `let`, a
comparison against `none`, and a `fail`. The gradient runs downhill toward
being wrong, which for an AI-first language is a design defect, not a
user error: the short program is the one that gets written.

**The second is a program bug the language could have prevented.** A body
that is not JSON leaves `req.data` as `none`, and `req.data.line` faults.
That is guardable — `if req.data == none` — and `quarry` now does.

**The third has no guard available at all.** A body that is valid JSON but
not an object faults on the first index, and *nothing in the language asks
which member of the union arrived*. `number(t)` answers "not a number"
with `none`. `json(t)` answers "not JSON" with `none`. There is no
equivalent for "is this a map", so a program cannot distinguish `{"a":1}`
from `[1,2,3]` before indexing it. The result is a 500 whose message
begins `internal:` — the runtime attributing to itself a condition the
caller chose.

That last point is a D3 violation of the same shape ADR-0025 records: the
condition is detectable at runtime, is not detected, and is not documented
in the reference as undetectable. It is also an A4 violation in spirit —
the wrongness surfaces, but as the server's fault rather than the
caller's.

## Decision

**A boundary needs a discriminator, and the language already has the
shape for one.** `number(t)` and `json(t)` establish the pattern:
a conversion that answers "not that shape" with `none` rather than
faulting. The same treatment for the composite members of `data` —
answering `none` when the value is not a map, or not a list — closes the
gap without adding a type-test construct, a pattern-match form, or any
new syntax. That keeps A6 (no surface extension) and spends a small,
countable amount of A1 budget on the one place where a program meets
input it did not write.

**A caller's malformed body is a 400, not a 500.** Where the runtime
faults on an operation whose operand came from the request, the status
belongs to the caller. This is narrower than it sounds and needs care —
the runtime cannot always trace a fault to caller input — which is why it
is scheduled rather than improvised.

**Until then, examples show the honest idiom.** `quarry` guards
`req.data == none`, parses with `number(...)` and refuses `none` rather
than defaulting it, range-checks, and counts every refusal on the board.
The one case it cannot guard is left as a 500 with a comment naming this
ADR, and its driving test asserts the 500 explicitly — an example that
quietly avoided the input it cannot handle would be hiding the finding
that justifies the work.

## Consequences

- The reference's §5 sentence that `data` is a union stays true; what is
  missing is a way to act on it, which this ADR names as open work.
- `t_examples_quarry_is_a_public_board_with_no_login` asserts the current
  behaviour of all three classes, including the 500. When the change
  lands, that assertion is the one to update — it says so in the test.
- The counted-refusals surface is worth keeping regardless of the language
  change: a status page that reports only the inputs it liked is
  describing its authors rather than its inputs.

## Resolution (2026-07-27)

**The discriminator landed: `fields(x)`.** For a map of data, the map; for
every other member of the union — list, number, text, bool, `none` — it
answers `none`. Total by construction, which is the point: a value you
cannot ask about is one you can only index and hope.

It answers `data?`, exactly as `json(t)` does, so what comes back is read
the same way the body would have been (`edit!["base"]`) rather than
through a second indexing idiom. Only the knowledge changed, not the
value. One row in §9.11, and one guard now subsumes three cases — a
missing body, a body that is not JSON, and a body that is JSON but not an
object all answer `none`, so `slate`'s routes replaced their
`req.data == none` check rather than adding to it.

**The second half is withdrawn, not deferred.** This ADR said a caller's
malformed body should be a 400 rather than a 500, and flagged that it
"needs care" because the runtime cannot always trace a fault to caller
input. Reading the dispatch path settles it more sharply than that:

> **Malformed is relative to the handler.** `[1,2,3]` is a perfectly good
> body for a route that wants a list. Nothing at the boundary knows which
> routes those are, so any rule that turns an `internal:` 500 into a 400
> because the body "looks wrong" is guessing — and a plausible-but-wrong
> 400 is exactly the laundering this ADR condemns in `number(text(x)) ?? 0`,
> moved from the program into the runtime.

The handler is the only thing that knows what it wanted. With the question
askable, it says so itself: `slate` refuses with its own `fail(400, ...)`,
and both the status and the message come from the code that knows. That is
better than the original proposal, not a lesser version of it — the 500 it
was trying to relabel no longer happens.

A 500 that *does* survive is now honest: it means a program indexed
something it never asked about, which is a program bug, and reporting it as
the server's fault is correct.

**What this does not close:** the ergonomic gradient. `number(text(x)) ?? 0`
is still the shortest thing that type-checks at a boundary, and it still
launders bad input into a plausible value. Rejecting still costs a `let`, a
comparison, and a `fail`. That is a separate finding about which programs
are cheap to write, and it stays open in the roadmap rather than being
quietly counted as fixed here.

## An upload is a path, not a byte member (2026-07-30)

A form carrying a file was the one body shape the runtime dropped on the floor:
`multipart/form-data` fell through to the JSON arm, failed to parse, and handed
the program `none`. So a page could offer a file picker and never receive what
was picked — and the enclave, told it could not have one, made people type a
path into the chat instead. The excuse was true and beside the point: a browser
cannot hand a server a path it can open, but a picker hands over the *file*.

The decoding is now there, and the question it forced is this ADR's: `data` has
text, number, bool, none, list and map, and an upload is bytes. Adding a seventh
member would touch every `fields` call, every shape check, every JSON
round-trip, and the union's whole claim to be *the shape of decoded payloads*.

So the runtime does the one thing only it can — get the bytes onto disk, under
the project's own runtime state — and the field is `{name, size, path}`. That
is not a workaround. What a program *does* with an upload is hand it on: to a
foreign store, to a `files` mount, to the mesh's `offer`. Every one of those
takes a path, and none of them wants bytes in an Ashlar value. `data` stays six
members wide.

Two details are load-bearing rather than tidy:

- **The saved name is the file's own.** Uniqueness lives in a directory per
  upload, not in the basename, because whatever is handed on carries that name
  onward — a peer in the room should see `plans.md`, not
  `1785369999557064730-0-plans.md`. The first cut got this wrong and the mesh
  showed the timestamp to the whole room.
- **The client's filename is never a path.** Only the last segment survives,
  and only characters that cannot climb out of a directory.

Drag and drop needed no language surface at all: the shim treats a drop on a
form that has a file input as choosing one, so the form already says where the
file goes and the program says nothing. The picker works with the shim switched
off, which is why the drop is the part that lives there. T-BROWSER drives both,
because neither a file dialog nor a `DataTransfer` exists without a browser.
