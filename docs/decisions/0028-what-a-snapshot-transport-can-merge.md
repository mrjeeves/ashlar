# ADR-0028: What a snapshot transport can merge, and what it cannot

Date: 2026-07-26

Status: accepted and applied (`examples/slate`)

## Context

`examples/slate` is a shared pad: open a URL, type, and everyone else on
that URL sees it. It exists because of one problem — two people typing at
once — and because that problem is not optional. A pad that loses a
sentence is not a pad.

The transport decides what is possible here. Ashlar's browser shim sends
an event carrying the field's **value**: not a keystroke, not an
operation, not a position — the whole text as it now reads. So the server
never learns "insert `x` at offset 40". It learns "this page says the pad
now reads THIS."

Take that at face value and you get last-write-wins, which is what every
naive version of this program does: the slower typist's snapshot lands on
top of the faster one's and a paragraph disappears with no error anywhere
in the system. It is silent, and it is the failure this example exists to
refuse.

## Decision

**Three texts, one answer.** Each page keeps `base` — the text it had in
front of it — as per-instance state (§9.4). An edit is therefore
`(base, incoming)` against the pad's `current`, and the merge is an
ordinary three-way merge, line by line:

| the writer's line | the pad's line | outcome |
|---|---|---|
| unchanged from base | anything | the pad's copy stands |
| changed | unchanged from base | the writer's line lands |
| changed, same as the pad's | — | agreement, either will do |
| changed, differs from the pad's | changed | **conflict**: the pad's copy stands, and the writer is told |

Line granularity is the honest unit for a snapshot transport. Character
granularity would need positions the client never sends, and document
granularity is last-write-wins wearing a hat.

**`base` moves with the screen.** When any edit lands, the pad publishes
its new text on a channel every open editor subscribes to (§9.5), and each
page sets `base` from it. The patch that updates a person's screen and the
message that updates their base are the same event; if they came apart,
every page would keep measuring new keystrokes against text the pad had
moved past, and ordinary edits would read as conflicts.

**A keystroke that crossed a patch is not an opinion.** There is still a
window: a finger comes down while someone else's line is in flight, so the
edit arrives carrying text that was current a moment ago. Taken literally
it silently undoes the other person's work — the exact failure we started
from, arriving by the back door. So a line that matches what the pad held
**one version ago** is treated as a page that has not caught up: the pad's
copy stands, and it is not reported as a conflict, because crying wolf at
every fast typist is its own failure.

One version back, and no further. That bound is the trade, stated plainly:

- **What it costs.** Editing a line back to exactly what it said one
  version ago is refused, because it is indistinguishable — with this
  transport — from a page that is a step behind. Change it to anything
  else, or do it once the pad has moved on again, and it lands.
- **What it buys.** A person typing at speed never destroys a colleague's
  sentence, which is the failure that makes people stop trusting a shared
  editor entirely.

**Conflicts are told, never swallowed.** The pad's copy winning is only
acceptable because the writer who lost hears about it on their own page.
A shared editor may drop an edit; it may not drop an edit in silence.

## Consequences

- The browser runs no editor code: no CRDT library, no operational
  transform shipped to the client, no diffing in JavaScript. The merge is
  ~40 lines of Ashlar on the server, and the client is the runtime's own
  shim. That is the claim this example makes for the language.
- `t_examples_slate_merges_two_people_typing_at_once` proves each rule on
  the path where it actually happens: live co-editing and the crossed
  keystroke over two real sockets, true concurrency over the HTTP edit
  route where two clients can state the base they were editing, and the
  deliberate later rewrite that must still land so the lag rule cannot
  freeze the text.
- The HTTP route takes `base` as a required part of an edit. A client that
  will not say what it was editing cannot be merged, only obeyed — so the
  API asks, and an absent base means "I was typing into a blank page,"
  which is treated as exactly that.
- If the shim ever sends selection offsets, this decision is revisitable
  and the merge could move to character granularity. It is written against
  the transport that exists, and says so.
