# ADR-0027: A subscriber's fault is that subscriber's

Date: 2026-07-26

Status: accepted and applied

## Context

`publish` delivered a message by walking the channel's handler list and
calling each one with `?`:

```rust
for h in handlers {
    self.call(h, vec![payload.clone()])?;
}
```

That `?` makes one subscriber's fault everyone's problem, twice over:

1. **Delivery stops, silently.** Every subscriber after the faulting one
   is skipped. Nothing is logged, nothing is returned, and the subscribers
   that were denied the message have no way to know — the channel simply
   goes quiet for them.
2. **The publisher is blamed.** The fault propagates out of `publish`, up
   through whatever called it, and ends that request with the failing
   handler's status. A visitor posting a sensor reading receives
   `500 division by zero.` — a fault raised in a different visitor's open
   page, in code the caller never invoked.

Reduced to a fixture (now `t_g_a_faulting_subscriber_does_not_take_the_channel_with_it`):
a page mounts two view parts, the first subscribing a handler that divides
by zero and the second a handler that counts. Before the fix, `/ring`
returned `500 division by zero.` and the counter never moved. Both
subscribers were correct code except one, and one was enough.

This is not a hypothetical failure mode for this runtime. Views subscribe
in their `start` stacks (§9.5), so the subscriber list is "every handler
of every instance every visitor currently has open" — a list the program's
author does not control and cannot inspect. Assuming all of them succeed
is assuming a clean room.

## Decision

**A fault in a subscriber is logged and delivery continues.** `publish`
calls every handler, records a structured error naming the channel and the
fault for any that fail, and returns normally to its caller.

This is not a new rule; it is an existing rule applied where it was
missing. §9.9 already settles the identical case for background work — "a
fault in it is logged, not fatal" — and a channel handler is the same kind
of thing: someone else's code, running because an event happened, on a
call path the publisher did not choose.

The alternatives were considered and rejected:

- **Unsubscribing a handler that faults.** Tempting, and wrong: a handler
  that faults on one message may be fine on the next (a `none` that was
  briefly absent), and silently removing a subscription is a worse
  surprise than a logged error, because nothing would ever say the page
  stopped listening.
- **Failing the publisher but continuing delivery.** Keeps the blame
  misplaced. The publisher did nothing wrong.
- **Collecting faults and returning them.** There is no catch construct
  (§9.9), so the caller could not act on them anyway.

## Consequences

- `reference/ashlar.md` §9.5 states the rule: handlers run in subscription
  order, a fault in one is logged, and it stops neither the others nor the
  caller. The reference grew by four lines; A1's budget is unaffected in
  practice (33,188 of 40,000 bytes).
- One regression test in T-G, written as the two properties that failed:
  the publisher's request survives, and the subscriber after the faulting
  one is still reached — on the second message as well as the first, so a
  fix that silently dropped the bad subscriber would not pass either.
- The structured log entry names the channel, so an operator seeing a
  quiet channel has something to search for. It is logged at `error`,
  since a subscriber that faults is a bug in that subscriber.
- Found by writing `examples/quarry`, whose alert channel is subscribed by
  a per-page ticker: several open pages is the normal state of a public
  status board, which is precisely the condition under which this bug
  bites and a single-client demo never would.
