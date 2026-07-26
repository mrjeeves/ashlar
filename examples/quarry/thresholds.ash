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
    return v
  }
}
