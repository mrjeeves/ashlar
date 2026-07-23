space guardrails.length
use guardrails.core

// This layer owns one concern. It can be added, removed, or refactored
// without editing the route or any other policy.
part guardrails.core.Gate {
  review pipe = (d: guardrails.core.Decision) => {
    let fits = len(d.body) <= 24
    return Gate.keep({
      body: d.body,
      allowed: d.allowed and fits,
      notes: if fits { d.notes } else { [...d.notes, "over 24 characters"] },
    })
  }
}
