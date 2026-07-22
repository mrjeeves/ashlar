## Correct reading

`subscribe("alerts", handler)` registers a handler for messages on the
"alerts" channel; `publish("alerts", {...})` sends a message to that
channel. The two are connected by the shared channel name. `start` runs
at startup and performs the subscription; `raise` publishes.

## Must state

- `subscribe("alerts", fn)` registers the given function to receive
  messages published on the `"alerts"` channel.
- `publish("alerts", { body: body })` sends a map message to that
  channel, reaching its subscribers.
- The publish and subscribe sides are connected by the shared channel
  name text `"alerts"`.
- `start` performs the subscription when the part starts up (the word
  `start` marks startup behavior); `raise` is a function that publishes a
  given body.
