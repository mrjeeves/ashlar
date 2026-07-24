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

part board {
  state entry: text = "2, 4, 4, 4, 5, 5, 7, 9"
  view = () => el("div", { class: "card" }, [
    el("p", { class: "kicker" }, ["worker transport · §9.10"]),
    el("h1", {}, ["abacus"]),
    el("p", { class: "lede" }, ["Every figure below comes from Python's statistics module, over a worker — ten lines, no compiler, no shared library."]),
    el("input", { class: "field", oninput: typed, value: entry }, []),
    el("ul", { class: "rows" }, rows(summarize(entry))),
  ])
  typed = (e: std.Event) => {
    entry = text(e.data.value)
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
  handle pipe = (req: std.Request) => summarize(text(req.data.entry))
}
