space quarry.ui
use quarry.api

// The board. Every view part here renders semantic HTML and carries only
// `class` names; the names meet assets/quarry.css by name (§9.4), and no
// element carries a style string.
//
// That last part is worth a sentence, because a status board is exactly
// where the temptation lands: a bar chart wants a computed height, and a
// height is a number. The answer here is to QUANTIZE the datum into a
// named bucket — `bar b7` — so appearance still binds by name and the
// sheet owns what `b7` looks like. Compare pong, whose ball genuinely is
// at pixel 195 and says so inline.

// The page: one shell, four regions, and a ticker that is fed by a
// channel rather than by anything on this page.
part board {
  focus: text
  view = () => el("div", { class: "app" }, [
    el(quarry.ui.banner, {}),
    el("main", { class: "main" }, panes()),
    el(quarry.ui.ticker, {}),
  ])
  panes = () => {
    if focus != "" {
      return [el(quarry.ui.detail, { key: focus })]
    }
    return [
      el(quarry.ui.grid, {}),
      el("div", { class: "cols" }, [el(quarry.ui.flow, {}), el(quarry.ui.journal, {})]),
      el(quarry.ui.wall, {}),
    ]
  }
}

// The one line at the top, and the only control on the page that changes
// how the server behaves: the report desk's shutter.
part banner {
  view = () => el("header", { class: "banner " + quarry.data.Store.headline() }, [
    el("div", { class: "id" }, [
      el("p", { class: "wordmark" }, ["quarry"]),
      el("p", { class: "tagline" }, ["public status · no account, no session"]),
    ]),
    el("div", { class: "state" }, [
      el("span", { class: "lamp " + quarry.data.Store.headline() }, []),
      el("span", { class: "word" }, [quarry.data.Store.headline()]),
    ]),
    el("dl", { class: "figures" }, figures()),
    el("div", { class: "desk" }, [
      el("button", { class: shutterClass(), onclick: toggle }, [shutterWord()]),
      el("a", { class: "manual", href: "/manual/fleet.json" }, ["fleet.json"]),
    ]),
  ])
  figures = () => [
    el("div", { class: "figure" }, [el("dt", {}, ["lines"]), el("dd", {}, [text(len(keys(quarry.data.Store.lines)))])]),
    el("div", { class: "figure" }, [el("dt", {}, ["tripped"]), el("dd", {}, [text(len(quarry.data.Store.tripped()))])]),
    el("div", { class: "figure" }, [el("dt", {}, ["at risk"]), el("dd", {}, [text(len(quarry.data.Store.atRisk()))])]),
    el("div", { class: "figure" }, [el("dt", {}, ["open"]), el("dd", {}, [text(len(quarry.data.Store.openIncidents()))])]),
    el("div", { class: "figure" }, [el("dt", {}, ["readings"]), el("dd", {}, [text(quarry.data.Store.pulse)])]),
  ]
  shutterWord = () => (if quarry.data.Store.intake { "reports: open" } else { "reports: closed" })
  shutterClass = () => (if quarry.data.Store.intake { "shutter open" } else { "shutter closed" })
  toggle = () => {
    quarry.data.Store.flip()
  }
}

part grid {
  view = () => el("section", { class: "grid" }, cards())
  cards = () => map(quarry.data.Store.fleet(), (l: quarry.data.Line) => el(quarry.ui.card, { key: l.key }))
}

// One line. Reading `levels`, `loads`, and `history` through the store's
// derived functions is what subscribes this instance to them: when the
// rig posts the next reading, only the cards that read a changed value
// re-render (§9.3).
part card {
  key: text
  view = () => el("article", { class: "card " + quarry.data.Store.levelOf(key) }, [
    el("header", { class: "cardhead" }, [
      el("a", { class: "name", href: "/line/" + key }, [quarry.data.Store.nameOf(key)]),
      el("span", { class: "stage" }, [stage()]),
    ]),
    el("p", { class: "load" }, [text(quarry.data.Store.loadOf(key))]),
    el(quarry.ui.spark, { key: key }),
    el("p", { class: "sub" }, [sub()]),
  ])
  stage = () => {
    let here = quarry.data.Store.lines[key]
    return if here != none { here!.stage } else { "" }
  }
  sub = () => {
    let n = len(quarry.data.Store.downstream(key))
    return if n == 0 { "supplies nothing downstream" } else { "supplies " + text(n) + " downstream" }
  }
}

// The sample history, drawn as ten named buckets. `bucket` is the whole
// trick: a number becomes a class name, and the sheet decides what that
// class looks like.
part spark {
  key: text
  view = () => el("div", { class: "spark" }, bars())
  bars = () => map(quarry.data.Store.sparkOf(key), (n: number) => el("span", { class: "bar b" + text(bucket(n)) }, []))
  bucket = (n: number) => {
    let tenth = n / 10
    let whole = tenth - tenth % 1
    return if whole > 9 { 9 } else if whole < 0 { 0 } else { whole }
  }
}

// The fleet graph, drawn from its roots. Neither the roots nor the depth
// are written down: both are computed from the `feeds` lists.
part flow {
  view = () => el("section", { class: "flow" }, [
    el("h2", {}, ["fleet"]),
    el("p", { class: "muted" }, ["Each tree starts at a line nobody feeds. A fault travels down."]),
    el("div", { class: "trees" }, trees()),
  ])
  trees = () => map(quarry.data.Store.roots(), (k: text) => el(quarry.ui.node, { key: k, depth: 6 }))
}

// A view part that instantiates ITSELF for its children. Across
// re-renders each child keeps its own instance by position (§9.4), so the
// tree is stable while the levels inside it change.
part node {
  key: text
  depth: number
  view = () => el("div", { class: "node" }, [
    el("a", { class: "twig " + quarry.data.Store.levelOf(key), href: "/line/" + key }, [
      el("span", { class: "dot" }, []),
      quarry.data.Store.nameOf(key),
    ]),
    el("div", { class: "kids" }, kids()),
  ])
  kids = () => {
    if depth <= 0 {
      return []
    }
    return map(quarry.data.Store.childrenOf(key), (k: text) => el(quarry.ui.node, { key: k, depth: depth - 1 }))
  }
}

part journal {
  view = () => el("section", { class: "journal" }, [
    el("h2", {}, ["incidents"]),
    el("ul", { class: "entries" }, entries()),
  ])
  entries = () => {
    let rows = quarry.data.Store.openIncidents() + quarry.data.Store.closedIncidents()
    return if len(rows) == 0 { [el("li", { class: "entry quiet" }, ["nothing to report"])] } else { map(slice(rows, 0, 8), (i: quarry.data.Incident) => el(quarry.ui.entry, {
      line: quarry.data.Store.nameOf(i.line),
      why: i.why,
      shut: i.closed != none,
    })) }
  }
}

part entry {
  line: text
  why: text
  shut: bool
  view = () => el("li", { class: cls() }, [
    el("span", { class: "tag" }, [word()]),
    el("span", { class: "what" }, [line + " — " + why]),
  ])
  cls = () => (if shut { "entry closed" } else { "entry open" })
  word = () => (if shut { "closed" } else { "open" })
}

// The public wall. Anyone reading the board can file a report; nothing
// here asks who they are, and nothing stores anything that would say.
part wall {
  state draft: text = ""
  state picked: text = ""
  view = () => el("section", { class: "wall" }, [
    el("h2", {}, ["from the floor"]),
    el("p", { class: "muted" }, [prompt()]),
    el("form", { class: "report", onsubmit: send }, [
      el("select", { class: "field pick", oninput: choose }, choices()),
      el("input", { class: "field grow", oninput: typed, value: draft, placeholder: "what do you see?" }, []),
      el("button", { class: "primary" }, ["file a report"]),
    ]),
    el("ul", { class: "notes" }, rows()),
  ])
  prompt = () => (if quarry.data.Store.intake { "The desk is open. No account needed." } else { "The desk is closed; reports are refused at the door." })
  choices = () => map(quarry.data.Store.fleet(), (l: quarry.data.Line) => el("option", attrsFor(l.key), [l.name]))
  attrsFor = (k: text) => (if k == chosen() { { value: k, selected: "selected" } } else { { value: k } })
  chosen = () => {
    if picked != "" {
      return picked
    }
    let first = quarry.data.Store.fleet()[0]
    return if first != none { first!.key } else { "" }
  }
  rows = () => {
    let notes = quarry.data.Store.recentNotes()
    return if len(notes) == 0 { [el("li", { class: "note quiet" }, ["no reports yet"])] } else { map(notes, (n: quarry.data.Note) => el("li", { class: "note" }, [
      el("span", { class: "on" }, [quarry.data.Store.nameOf(n.line)]),
      el("span", { class: "said" }, [n.body]),
    ])) }
  }
  typed = (e: std.Event) => {
    draft = text(e.data.value)
  }
  choose = (e: std.Event) => {
    picked = text(e.data.value)
  }
  send = () => {
    quarry.data.Store.record(chosen(), draft)
    draft = ""
  }
}

// The ticker holds nothing the page could have told it. Its list is
// per-instance state fed only by the subscription opened in its `start`
// stack (§9.5), so a board that has been open for an hour shows what it
// heard, and a board opened a second ago shows nothing.
part ticker {
  state heard: [text] = []
  start stack = () => {
    subscribe(quarry.alerts.Feed.channel, noted)
    return none
  }
  noted = (m: data) => {
    heard = [...heard, text(m)]
  }
  view = () => el("aside", { class: "ticker" }, [
    el("p", { class: "tickerhead" }, ["alerts heard on this page"]),
    el("ul", { class: "heard" }, items()),
  ])
  items = () => {
    if len(heard) == 0 {
      return [el("li", { class: "quiet" }, ["nothing since you opened this page"])]
    }
    let n = len(heard)
    let from = if n > 4 { n - 4 } else { 0 }
    return map(slice(heard, from, n), (h: text) => el("li", {}, [h]))
  }
}

// One line, in full: its samples, everything it supplies, and what the
// floor has said about it.
part detail {
  key: text
  view = () => el("section", { class: "detail" }, [
    el("a", { class: "back", href: "/" }, ["← the whole board"]),
    el("h2", {}, [quarry.data.Store.nameOf(key)]),
    el("p", { class: "level " + quarry.data.Store.levelOf(key) }, [quarry.data.Store.levelOf(key) + " · load " + text(quarry.data.Store.loadOf(key))]),
    el(quarry.ui.spark, { key: key }),
    el("h3", {}, ["downstream"]),
    el("div", { class: "trees" }, [el(quarry.ui.node, { key: key, depth: 6 })]),
    el("h3", {}, ["reports"]),
    el("ul", { class: "notes" }, said()),
  ])
  said = () => {
    let mine = quarry.data.Store.notesOn(key)
    return if len(mine) == 0 { [el("li", { class: "note quiet" }, ["nothing reported on this line"])] } else { map(mine, (n: quarry.data.Note) => el("li", { class: "note" }, [el("span", { class: "said" }, [n.body])])) }
  }
}
