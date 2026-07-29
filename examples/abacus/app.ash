space abacus

part app {
  port = 8080
  style = "abacus"
}

// A data shape the boundary checks every answer against (§9.10): if the
// worker ever returned a row that stopped matching, it would fault at the
// call site rather than slip through as bad data.
part Summary {
  mean: number
  median: number
  spread: number
}

// The capability, named here and implemented in Python. Its binding lives in
// foreign.json — `via: worker`, a co-process speaking JSON Lines — so there is
// no shared library, no C ABI, and no compiler anywhere in this project
// (ADR-0017). Parsing the messy input is left to the language that is good at
// it; Ashlar names the capability and checks the shape.
foreign summarize: (entry: text) -> Summary

part page {
  route = "/"
  view = () => el("div", { class: "stage" }, [el(board, {})])
}

// A bench everybody adds to. The worker is a co-process for the whole
// program, not for a page, so one shared list summarized once is the honest
// shape: a number added here is summarized for every window (§9.3), and the
// figures still cross the boundary shape-checked.
part Bench {
  stored samples: [number] = []
  add = (n: number) => {
    samples = [...samples, n]
  }
  wipe = () => {
    samples = []
  }
  written = () => join(map(samples, (n: number) => text(n)), ", ")
}

part board {
  state entry: text = "2, 4, 4, 4, 5, 5, 7, 9"
  state draft: text = ""
  view = () => el("div", { class: "card" }, [
    el("title", {}, ["abacus"]),
    el("p", { class: "kicker" }, ["worker transport · §9.10"]),
    el("h1", {}, ["abacus"]),
    el("p", { class: "lede" }, ["Every figure below comes from Python's statistics module, over a worker — ten lines, no compiler, no shared library."]),
    el("p", { class: "panetitle" }, ["your scratch"]),
    el("input", { class: "field", oninput: typed, value: entry, autocomplete: "off" }, []),
    el("ul", { class: "rows" }, rows(summarize(entry))),
    el("p", { class: "panetitle" }, ["the shared bench"]),
    el("form", { class: "row-in", onsubmit: contribute }, [
      el("input", { class: "field", oninput: drafted, value: draft, placeholder: "add a number for everybody", autocomplete: "off" }, []),
      el("button", { class: "quiet" }, ["add"]),
    ]),
  ] + bench())

  // The bench is only summarized when there is something on it: an empty
  // sample has no mean, and asking for one is the caller's mistake to avoid,
  // not the worker's to invent.
  bench = () => (if len(Bench.samples) == 0 { [el("p", { class: "none" }, ["Nothing on the bench. Add a number and every window on this page gets the new figures."])] } else { [
    el("p", { class: "samples" }, [Bench.written()]),
    el("ul", { class: "rows" }, rows(summarize(Bench.written()))),
    el("button", { class: "wipe", onclick: clear }, ["clear the bench"]),
  ] })

  typed = (e: std.Event) => {
    entry = text(e.data.value)
  }
  drafted = (e: std.Event) => {
    draft = text(e.data.value ?? "")
  }
  contribute = () => {
    let n = number(draft)
    if n != none {
      Bench.add(n!)
    }
    draft = ""
  }
  clear = () => {
    Bench.wipe()
  }
  rows = (s: abacus.Summary) => [
    el("li", { class: "row" }, ["mean " + text(s.mean)]),
    el("li", { class: "row" }, ["median " + text(s.median)]),
    el("li", { class: "row" }, ["spread " + text(s.spread)]),
  ]
}

// The same capability over HTTP, so a client gets it too (§9.2).
part api {
  route = "/api/summary"
  handle pipe = (req: std.Request) => {
    // Everything from outside is `data`, a union (§5). `fields` is the
    // question that keeps a caller's malformed body from becoming the
    // server's 500 (ADR-0026).
    let d = fields(req.data) ?? fail(400, "send a JSON object: { entry }")
    return summarize(text(d["entry"] ?? ""))
  }
}
