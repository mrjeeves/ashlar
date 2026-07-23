space guardrails.secrets
use guardrails.length

// The use edge makes policy order explicit: length runs before secrets.
// Both layers preserve the same Decision shape, so disagreement is a
// compile error instead of a runtime surprise.
part guardrails.core.Gate {
  review pipe = (d: guardrails.core.Decision) => {
    let clean = not contains(d.body, "secret")
    return Gate.keep({
      body: d.body,
      allowed: d.allowed and clean,
      notes: if clean { d.notes } else { [...d.notes, "contains secret"] },
    })
  }
}
