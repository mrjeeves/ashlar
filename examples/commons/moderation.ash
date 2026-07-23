space commons.moderation
use commons.data

// An independently owned space. It layers the store's `prepare` pipe —
// the seam every message body passes through — to redact a blocklist,
// without editing commons.data. Composition is `use` order: the base
// clamp runs first, then this redaction receives its result (§4). Delete
// this file and posting still works; the policy simply lifts off.
part commons.data.Store {
  prepare pipe = (body: text) => join(split(body, "spoiler"), "[redacted]")
}
