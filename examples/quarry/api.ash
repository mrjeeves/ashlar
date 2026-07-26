space quarry.api
use quarry.sensors

// The machine-readable board. Every route here is open: no session, no
// `allow = (req) => req.user != none`, nothing to sign into. A status
// page whose status you need an account to read is not a status page.

part fleet {
  route = "/api/lines"
  handle pipe = (req: std.Request) => {
    return {
      headline: quarry.data.Store.headline(),
      pulse: quarry.data.Store.pulse,
      // What this program was sent and would not accept. A status page
      // that reports only the inputs it liked is describing its authors.
      refused: quarry.data.Store.refused,
      last_refusal: quarry.data.Store.lastRefusal,
      // Which policies are in force, in the order they run. The list is
      // an `append` property four spaces contribute to, so this answer is
      // assembled by the use graph rather than written anywhere (§4).
      policies: quarry.data.Store.tags,
      lines: map(quarry.data.Store.fleet(), (l: quarry.data.Line) => quarry.api.fleet.row(l)),
    }
  }

  row = (l: quarry.data.Line) => {
    return {
      key: l.key,
      name: l.name,
      stage: l.stage,
      level: quarry.data.Store.levelOf(l.key),
      load: quarry.data.Store.loadOf(l.key),
      feeds: l.feeds,
      downstream: len(quarry.data.Store.downstream(l.key)),
    }
  }
}

// A path that names something the fleet does not have is a 404 — the one
// case where the answer is a status code rather than a shape (§9.2).
part oneLine {
  route = "/api/line/{key}"
  handle pipe = (req: std.Request) => {
    let here = quarry.data.Store.lines[req.params["key"]!]
    if here == none {
      return fail(404, "no such line")
    }
    return {
      line: quarry.api.fleet.row(here!),
      spark: quarry.data.Store.sparkOf(here!.key),
      notes: len(quarry.data.Store.notesOn(here!.key)),
      at_risk: quarry.data.Store.downstream(here!.key),
    }
  }
}

// The ingest path a sensor posts to. The rig on the floor calls the same
// `observe`, so this is not a test hatch bolted to the side — it is the
// only door, and the schedule uses it too.
//
// It is also the place this example is actually about. Everything that
// arrives from outside is `data` (§5) — a union of text, number, bool,
// none, list, and map — and a program does not get to assume which one
// turned up. The short idiom is the dangerous one: `number(text(req.data.load)) ?? 0`
// type-checks, accepts "banana", and records a reading of zero that
// nothing downstream can tell from a real one. `??` at a boundary
// launders bad input into a plausible value. So this handler refuses
// instead, and every refusal is counted where the board can show it: a
// status page that hides how much nonsense it is sent is telling you
// about its authors, not its fleet.
//
// One case it CANNOT handle is left visible on purpose. A body that is
// valid JSON but not an object — `[1,2,3]`, `42`, `"hello"` — faults on
// the first index and ends as a 500 reading `internal: cannot index a
// list with text`. Nothing in the language asks which member of `data`
// arrived: `number(t)` and `json(t)` answer "not that shape" with `none`,
// and there is no such conversion for a map. See ADR-0026.
part reading {
  route = "/api/observe"
  handle pipe = (req: std.Request) => {
    if req.data == none {
      quarry.data.Store.refuse("a body that was not JSON")
      return fail(400, "a reading is a JSON object: { line, load }")
    }
    let key = text(req.data["line"] ?? "")
    if quarry.data.Store.lines[key] == none {
      quarry.data.Store.refuse("unknown line " + key)
      return fail(404, "no such line")
    }
    let load = number(text(req.data["load"] ?? ""))
    if load == none {
      quarry.data.Store.refuse("load was not a number, on " + key)
      return fail(400, "`load` must be a number")
    }
    if load! < 0 or load! > 100 {
      quarry.data.Store.refuse("load out of range, on " + key)
      return fail(400, "`load` is a percentage of capacity: 0 to 100")
    }
    quarry.data.Store.observe(key, load!)
    return {
      line: key,
      level: quarry.data.Store.levelOf(key),
      load: quarry.data.Store.loadOf(key),
      at_risk: quarry.data.Store.atRisk(),
    }
  }
}

// The public report desk, and the reason `allow` is not an auth feature.
// The guard here asks nothing about WHO is calling — there is nobody to
// ask about. It asks whether the desk is open, which is program state the
// board can flip, and a closed desk ends the request with 403 before
// `handle` runs (§9.6). Authorization without identity is still
// authorization.
part desk {
  route = "/api/report"
  allow = (req: std.Request) => quarry.data.Store.intake
  handle pipe = (req: std.Request) => {
    if req.data == none {
      quarry.data.Store.refuse("a report body that was not JSON")
      return fail(400, "a report is a JSON object: { line, body }")
    }
    let key = text(req.data["line"] ?? "")
    if quarry.data.Store.lines[key] == none {
      quarry.data.Store.refuse("report about unknown line " + key)
      return fail(404, "no such line")
    }
    // An empty report used to redirect like a successful one, because
    // `record` drops empties — a 302 for work that did not happen. The
    // caller is told.
    let said = text(req.data["body"] ?? "")
    if said == "" {
      quarry.data.Store.refuse("empty report on " + key)
      return fail(400, "`body` is what you saw; an empty one is not a report")
    }
    quarry.data.Store.record(key, said)
    return redirect("/")
  }
}

part shutter {
  route = "/api/intake"
  handle pipe = (req: std.Request) => {
    quarry.data.Store.flip()
    return { intake: quarry.data.Store.intake }
  }
}

// The blast radius of a fault, over HTTP. It is the same walk the board
// draws and the same walk `atRisk` uses — computed from the graph, never
// listed by hand.
part spread {
  route = "/api/impact/{key}"
  handle pipe = (req: std.Request) => {
    let key = req.params["key"]!
    if quarry.data.Store.lines[key] == none {
      return fail(404, "no such line")
    }
    return { line: key, downstream: quarry.data.Store.downstream(key) }
  }
}
