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
part reading {
  route = "/api/observe"
  handle pipe = (req: std.Request) => {
    let key = text(req.data.line)
    if quarry.data.Store.lines[key] == none {
      return fail(404, "no such line")
    }
    quarry.data.Store.observe(key, number(text(req.data.load)) ?? 0)
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
    let key = text(req.data.line)
    if quarry.data.Store.lines[key] == none {
      return fail(404, "no such line")
    }
    quarry.data.Store.record(key, text(req.data.body))
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
