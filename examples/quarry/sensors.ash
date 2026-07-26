space quarry.sensors
use quarry.alerts

// The rig. A scheduled part (§9.7) walks the fleet and feeds a reading
// into the same `observe` an HTTP client posts to — the schedule has no
// privileged path in (§9.2).
//
// The numbers are made up, but they are made up DETERMINISTICALLY: a
// linear congruential step, whose every intermediate stays well inside
// the exact-integer range, so the same board on two machines tells the
// same story and a test can assert on it. There is no random builtin,
// which is the correct answer for a language whose reference is the whole
// surface — randomness is a capability, and capabilities go through
// `foreign` (§9.10) or through arithmetic you can read.
part Rig {
  state seed: number = 20260726
  state ticks: number = 0

  every = "500ms"

  // Only the instrumented lines. The crating bench and the loading dock
  // have no sensor on them — their numbers arrive when someone posts
  // them, through the same `/api/observe` this rig uses. A fleet where
  // every reading is automatic is a fleet nobody has worked in.
  sensors = ["yard", "saw", "kiln", "polish", "edge"]

  run = () => {
    ticks = ticks + 1
    for key in sensors {
      quarry.data.Store.observe(key, next(key))
    }
    if ticks % 20 == 0 {
      spawn(() => sweep())
    }
  }

  // One step of the generator per line, mixed with a per-line offset so
  // the lines do not all breathe together.
  next = (key: text) => {
    seed = (seed * 75 + 74) % 65537
    let base = seed % 40
    return base + lean(key)
  }

  // The saw runs hot. It rarely crosses the trip mark on one reading —
  // what takes it down is leaning on the warn mark three readings
  // running, which is the streaks layer's decision, not the thresholds
  // layer's. Watch the board long enough and you will see it happen.
  lean = (key: text) => {
    let here = quarry.data.Store.lines[key]
    return if here != none and here!.stage == "cut" { 40 } else { 4 }
  }

  // Housekeeping between requests (§9.7). It is `spawn`ed rather than run
  // inline because nothing waits on it, and a fault inside a spawned
  // function is logged rather than fatal.
  sweep = () => {
    log.debug("quarry: sweep", {
      lines: len(keys(quarry.data.Store.lines)),
      open: len(quarry.data.Store.openIncidents()),
    })
  }
}
