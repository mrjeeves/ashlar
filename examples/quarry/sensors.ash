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

  // A reading is a WALK from the last one, not a fresh draw. The first
  // version of this rig sampled a uniform range each tick, which reads as
  // plausible on a screenshot and is wrong in the way that matters: a
  // machine that was at 70 a half-second ago is not equally likely to be
  // at 12 now. Independent draws also make every threshold decision a
  // coin flip, so the board flickers and the alert channel fills with a
  // simulation's noise rather than a fleet's news.
  //
  // So: small step, mean-reverting toward a per-stage resting load. The
  // saw rests near the warn mark and wanders over it; everything else
  // rests low. Runs of high readings now actually happen, which is what
  // the streaks layer is for, and recovery takes a while, which is what
  // the recovery mark is for.
  next = (key: text) => {
    seed = (seed * 75 + 74) % 65537
    let step = seed % 11 - 5
    let here = quarry.data.Store.loadOf(key)
    let pull = if here < resting(key) { 2 } else { -2 }
    return held(here + step + pull)
  }

  resting = (key: text) => {
    let here = quarry.data.Store.lines[key]
    return if here != none and here!.stage == "cut" { 58 } else { 24 }
  }

  // A load is a percentage of capacity, and the ingest path refuses
  // anything outside 0..100 — including from this rig, which gets no
  // special door (§9.2). Keeping the walk inside the range is the rig's
  // job, not the boundary's.
  held = (n: number) => {
    if n < 2 {
      return 2
    }
    return if n > 98 { 98 } else { n }
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
