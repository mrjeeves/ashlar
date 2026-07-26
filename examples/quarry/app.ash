space quarry.app
use quarry.ui

// The server root, and the last space in the chain: quarry.app uses
// quarry.ui uses quarry.api uses quarry.sensors uses quarry.alerts uses
// quarry.streaks uses quarry.thresholds uses quarry.data. That chain is
// the whole ordering story — eight spaces, one composition order, and no
// file said where anything lives (§3).
part app {
  port = 8080
  style = "quarry"

  // The boot sequence is a `stack` that runs base-first, so the store
  // comes up before the alerts layer arms, and `wind` — declared
  // `reverse` — takes them down the other way (§4, §9.1).
  start stack = () => {
    seed()
    quarry.data.Store.boot()
    log.info("quarry: board up", { tags: quarry.data.Store.tags })
    return none
  }

  stop stack reverse = () => {
    quarry.data.Store.wind()
    return none
  }

  // The fleet, as a graph. `feeds` is the only edge list; roots, depth,
  // and blast radius are all computed from it and never written down.
  seed = () => {
    quarry.data.Store.ensureLine("yard", "the yard", "intake", ["saw"])
    quarry.data.Store.ensureLine("saw", "primary saw", "cut", ["polish", "edge"])
    quarry.data.Store.ensureLine("kiln", "drying kiln", "cure", ["polish"])
    quarry.data.Store.ensureLine("polish", "polish line", "finish", ["crate"])
    quarry.data.Store.ensureLine("edge", "edging line", "finish", ["crate"])
    quarry.data.Store.ensureLine("crate", "crating", "pack", ["dock"])
    quarry.data.Store.ensureLine("dock", "loading dock", "ship", [])
  }
}

part page {
  route = "/"
  view = () => el(quarry.ui.board, { focus: "" })
}

// The same board, focused on one line. The route's capture crosses into
// the view as a field (§9.4) — the whole of the plumbing.
part linePage {
  route = "/line/{key}"
  handle pipe = (req: std.Request) => el(quarry.ui.board, { focus: req.params["key"]! })
}

// Static assets (§9.8): the fleet layout as a file anyone can fetch, so
// a scraper does not have to read HTML. The value names a directory under
// `assets/`; the build records where that actually is (§10), and the
// route prefix is this part's own.
part manual {
  route = "/manual"
  files = "manual"
}
