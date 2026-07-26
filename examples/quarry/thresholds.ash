space quarry.thresholds
use quarry.data

// The first policy layer, owned by whoever owns thresholds. It never
// edits quarry.data: it declares a layer on that store's `classify` seam
// and receives the base verdict (§4).

// Where the numbers come from is a deployment fact, not a fact about the
// program (§9.12, B5). Both carry defaults, so the board runs with none
// supplied; settings.json overrides them without touching a line of this.
part Policy {
  setting warn: number = 60
  setting trip: number = 85

  // Recovery is not the warn mark read backwards. A line hovering at the
  // threshold crosses it constantly, and without a gap between "it
  // tripped" and "it is better" the board opens an incident every other
  // reading and the alert stream becomes noise nobody reads. This is the
  // first thing running the example for ten minutes teaches, and it is
  // not the sort of thing a screenshot would have shown.
  setting recover: number = 45
}

part quarry.data.Store {
  tags append = ["thresholds"]
  limits deep = { load: { floor: 0, ceiling: 100 } }

  classify pipe = (v: quarry.data.Verdict) => {
    if v.load >= quarry.thresholds.Policy.trip {
      return keep({
        line: v.line,
        load: v.load,
        level: "tripped",
        why: [...v.why, "load " + text(v.load) + " at or over " + text(quarry.thresholds.Policy.trip)],
      })
    }
    if v.load >= quarry.thresholds.Policy.warn {
      return keep({
        line: v.line,
        load: v.load,
        level: "strained",
        why: [...v.why, "load " + text(v.load) + " at or over " + text(quarry.thresholds.Policy.warn)],
      })
    }
    // Hysteresis. A line between the recovery mark and the warn mark is
    // not steady yet: it is coming down, or hovering. Saying so here is
    // what stops an incident opening and closing on alternate readings,
    // and it keeps the whole decision — trip, warn, recover — in the one
    // space that owns it.
    if v.load > quarry.thresholds.Policy.recover {
      return keep({
        line: v.line,
        load: v.load,
        level: "strained",
        why: [...v.why, "load " + text(v.load) + " still above the recovery mark of " + text(quarry.thresholds.Policy.recover)],
      })
    }
    return v
  }
}
