## Correct reading

`alerts` has `port`, so it is the server root; `start` runs once at launch
and calls `subscribe("alerts", handler)`, registering `handler` to run
whenever a message is published on the `"alerts"` channel. Because `alerts`
is not a view part, this subscription lives for the whole process, not a
per-instance mount. `raise` calls `publish("alerts", { body: body })`,
sending a message to every current subscriber of that channel. Channel names
are plain runtime text, matched at runtime, not resolved at build time.

## Must state

- `subscribe("alerts", handler)` inside `start` registers `handler` to be
  called whenever a message is published on the `"alerts"` channel; since
  `alerts` is not a view part, the subscription lives for the whole process,
  not a per-instance mount.
- `publish("alerts", { body: body })` inside `raise` sends a message to every
  current subscriber of that same channel name.
- `alerts` has `port`, making it the server root that `ashlar run` starts;
  that is why `start` (and thus the `subscribe` call) runs automatically
  once, at launch.
- `publish` and `subscribe` are connected only by the channel name string
  itself (runtime data, not a program name) — there is no other,
  build-time-checked link between this `publish` call and this `subscribe`
  call.
