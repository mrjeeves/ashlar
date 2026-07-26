space quarry.data

// quarry — the public status board for a stone works. Nobody signs in:
// there is no account, no session, and no `peruser` anything. Every
// visitor sees the same board, and anyone may file a report. What makes
// it complex is composition, not identity — five spaces layer this one
// store, and the fleet is a graph whose faults travel downstream.

// The domain shapes. A data shape is a part of typed fields only; its
// values are plain literals checked against the fields wherever one is
// built (§5).

// One cutting line. `feeds` names the lines this one supplies, so the
// fleet is a directed graph written as data — which is why the walks
// below carry fuel: the compiler rules out cycles in the USE graph, but
// this graph is values, and values are the program's own to trust.
part Line {
  key: text
  name: text
  stage: text
  feeds: [text]
}

// A reading on its way through the policy seam. `classify` is a `pipe`,
// so every space that layers it receives the previous layer's verdict and
// returns one of the same shape — disagreement is a compile error, not a
// surprise at 3am (§4).
part Verdict {
  line: text
  load: number
  level: text
  why: [text]
}

part Incident {
  ref: text
  line: text
  level: text
  why: text
  opened: number
  closed: number?
}

// A note from whoever is looking at the board. No author, no account —
// the anonymity is the point.
part Note {
  ref: text
  line: text
  body: text
  at: number
}

// The one store every other space reads, and the only place state lives.
// Two axes, both visible here: lifetime (`stored` on disk, `state` in
// memory) and — deliberately unused — scope, because a public board has
// no user to scope anything to.
//
// The seams are the interesting part. `classify` and `announce` are
// `pipe` properties, `boot`/`wind` a paired `stack`/`stack reverse`,
// `tags` an `append` and `limits` a `deep`: all five merge kinds, layered
// by four other spaces that never edit this file.
part Store {
  stored lines: {text: quarry.data.Line} = {}
  stored incidents: {text: quarry.data.Incident} = {}
  stored notes: [quarry.data.Note] = []
  stored intake: bool = true

  // Telemetry is memory, on purpose: a load reading is true for a second
  // and worth nothing after a restart. Incidents and notes are the record,
  // so those are `stored`.
  state loads: {text: number} = {}
  state history: {text: [number]} = {}
  state levels: {text: text} = {}
  state pulse: number = 0

  tags append: [text] = ["core"]
  limits deep: {text: {text: number}} = { samples: { keep: 12 }, walk: { fuel: 64 } }

  // The policy seam. The base pass decides nothing: it hands the verdict
  // on unchanged, so that deleting every policy space leaves a board that
  // still runs and simply never complains.
  classify pipe = (v: quarry.data.Verdict) => v

  // `keep` exists to give a literal a shape, and it should not have to.
  // A literal is checked against the shape its position expects (§5), and
  // an argument is such a position — but a `return` is not one yet, so a
  // layer that returns a correct Verdict literal is rejected for returning
  // a map. guardrails carries the same property for the same reason. Both
  // come out when ADR-0025 lands; until then this is the workaround, said
  // plainly rather than dressed as a design.
  keep = (v: quarry.data.Verdict) => v

  // The reaction seam. An incident that just opened passes through here
  // on its way to whoever is listening.
  announce pipe = (i: quarry.data.Incident) => i

  boot stack = () => {
    log.info("quarry: store online")
    return none
  }

  wind stack reverse = () => {
    log.info("quarry: store down")
    return none
  }

  // -- writes ---------------------------------------------------------

  ensureLine = (key: text, name: text, stage: text, feeds: [text]) => {
    if lines[key] == none {
      lines = put(lines, key, { key: key, name: name, stage: stage, feeds: feeds })
    }
  }

  // The one ingest path. A reading arrives (from the rig, from the HTTP
  // API — the handler cannot tell, §9.2), runs the composed policy, and
  // whatever the layers decided is what gets recorded.
  observe = (key: text, load: number) => {
    if lines[key] != none {
      loads = put(loads, key, load)
      history = put(history, key, trimmed(history[key] ?? [], load))
      let v = classify({ line: key, load: load, level: "steady", why: [] })
      levels = put(levels, key, v.level)
      pulse = pulse + 1
      mark(v)
    }
  }

  // Only the last few samples are worth keeping: the board draws them and
  // the streak policy reads them, and neither wants a growing list.
  trimmed = (samples: [number], load: number) => {
    let all = samples + [load]
    let cap = limits["samples"]!["keep"]!
    return if len(all) > cap { slice(all, len(all) - cap, len(all)) } else { all }
  }

  // An incident opens when a line trips and stays open until it is steady
  // again, so a line that flaps does not open a hundred of them.
  mark = (v: quarry.data.Verdict) => {
    let open = openOn(v.line)
    if v.level == "tripped" and open == none {
      let ref = id()
      incidents = put(incidents, ref, {
        ref: ref,
        line: v.line,
        level: v.level,
        why: join(v.why, " · "),
        opened: now(),
        closed: none,
      })
      announce(incidents[ref]!)
    }
    if v.level == "steady" and open != none {
      incidents = put(incidents, open!.ref, { ...open!, closed: now() })
    }
  }

  // A report from a visitor. Anyone can file one; nothing here asks who.
  record = (key: text, body: text) => {
    let short = slice(body, 0, 160)
    if short != "" {
      notes = [...notes, { ref: id(), line: key, body: short, at: now() }]
    }
  }

  flip = () => {
    intake = not intake
  }

  // -- derived reads --------------------------------------------------
  //
  // Views call these, so the reads they perform on `lines`, `levels`, and
  // the rest are exactly what makes those views reactive (§9.3).

  levelOf = (key: text) => levels[key] ?? "steady"

  loadOf = (key: text) => loads[key] ?? 0

  sparkOf = (key: text) => history[key] ?? []

  nameOf = (key: text) => {
    let here = lines[key]
    return if here != none { here!.name } else { key }
  }

  childrenOf = (key: text) => {
    let here = lines[key]
    return if here != none { here!.feeds } else { [] }
  }

  fleet = () => sort(map(keys(lines), (k: text) => lines[k]!), (l: quarry.data.Line) => l.stage + l.name)

  // A line nobody feeds is where the graph starts. The board draws one
  // tree per root, so the whole fleet appears without a written order.
  roots = () => filter(keys(lines), (k: text) => len(filter(keys(lines), (up: text) => contains(childrenOf(up), k))) == 0)

  // Breadth-first over the fleet graph, recursion among named functions
  // (§7). Locals are single-assignment, so there is no accumulator to
  // mutate: the frontier and the answer are both parameters, and each
  // call rebinds them. `fuel` bounds the walk — the visited check already
  // terminates on a cycle, and the fuel says so out loud for a graph that
  // arrives as data.
  walk = (pending: [text], found: [text], fuel: number) => {
    if fuel <= 0 or len(pending) == 0 {
      return found
    }
    let head = pending[0]!
    let rest = slice(pending, 1, len(pending))
    return if contains(found, head) { walk(rest, found, fuel - 1) } else { walk(rest + childrenOf(head), found + [head], fuel - 1) }
  }

  downstream = (key: text) => walk(childrenOf(key), [], limits["walk"]!["fuel"]!)

  tripped = () => filter(keys(lines), (k: text) => levelOf(k) == "tripped")

  // Everything a tripped line supplies, minus the tripped lines
  // themselves: the blast radius of the fault, computed the same way the
  // toolchain computes a rename's (§12) — from the graph, not by hand.
  atRisk = () => filter(walk(tripped(), [], limits["walk"]!["fuel"]!), (k: text) => not contains(tripped(), k))

  openIncidents = () => sort(filter(map(keys(incidents), (k: text) => incidents[k]!), (i: quarry.data.Incident) => i.closed == none), (i: quarry.data.Incident) => 0 - i.opened)

  closedIncidents = () => sort(filter(map(keys(incidents), (k: text) => incidents[k]!), (i: quarry.data.Incident) => i.closed != none), (i: quarry.data.Incident) => 0 - i.opened)

  openOn = (key: text) => find(map(keys(incidents), (k: text) => incidents[k]!), (i: quarry.data.Incident) => i.line == key and i.closed == none)

  notesOn = (key: text) => filter(notes, (n: quarry.data.Note) => n.line == key)

  recentNotes = () => {
    let n = len(notes)
    return if n > 6 { slice(notes, n - 6, n) } else { notes }
  }

  // The one word at the top of the page.
  headline = () => {
    return if len(tripped()) > 0 { "tripped" } else if len(filter(keys(lines), (k: text) => levelOf(k) == "strained")) > 0 { "strained" } else { "steady" }
  }
}
