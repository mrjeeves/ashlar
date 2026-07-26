space quarry.streaks
use quarry.thresholds

// The second policy layer, and the reason `use` is the ordering
// mechanism. It reads a verdict the thresholds layer already decided and
// escalates a line that has been leaning on the limit for a while — which
// is only meaningful AFTER a level exists, so this space uses
// quarry.thresholds and the compiler puts the layers in that order (§3).
//
// It reads the earlier layer's `warn` mark by name — the `use` that
// orders the two layers is the same `use` that puts the name in scope, so
// there is no second place to configure and no copied number to drift.
part Escalation {
  setting streak: number = 3
}

part quarry.data.Store {
  tags append = ["streaks"]

  classify pipe = (v: quarry.data.Verdict) => {
    let run = leaning(v.line)
    if v.level == "strained" and run >= quarry.streaks.Escalation.streak {
      return keep({
        line: v.line,
        load: v.load,
        level: "tripped",
        why: [...v.why, "strained on " + text(run) + " straight readings"],
      })
    }
    return v
  }

  // How many of the most recent samples are at or over the strained mark,
  // counted from the newest backwards. Recursion again, because a local
  // cannot be a counter — and the shape reads better for it: the answer
  // for a list is the answer for its tail, plus one.
  leaning = (key: text) => {
    let samples = sparkOf(key)
    return leadingRun(samples, len(samples) - 1, 0)
  }

  leadingRun = (samples: [number], at: number, so_far: number) => {
    if at < 0 {
      return so_far
    }
    let here = samples[at]
    return if here != none and here! >= quarry.thresholds.Policy.warn { leadingRun(samples, at - 1, so_far + 1) } else { so_far }
  }
}
