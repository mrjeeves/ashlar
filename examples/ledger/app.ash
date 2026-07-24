space ledger
use ledger.store

part app {
  port = 8080
  style = "ledger"
}

// The board renders the whole ledger from SQLite on every request: the
// entries newest-first, and the running total, which SQL sums in the shim
// (ledger.store.total) — Ashlar composes, the database aggregates. There
// is no reactive `stored` here, so the page is plain request/response;
// making a foreign store live is the next stage (ADR-0014).
part board {
  route = "/"
  view = () => el("main", { class: "ledger" }, [
    el("h1", {}, ["Ledger"]),
    el("form", { method: "post", action: "/add", class: "add" }, [
      el("input", { name: "who", placeholder: "who" }, []),
      el("input", { name: "note", placeholder: "for what" }, []),
      el("input", { name: "amount", placeholder: "amount" }, []),
      el("button", {}, ["record"]),
    ]),
    el("ul", { class: "rows" }, map(recent(), (e: Entry) => el("li", { class: "row" }, [e.who + ": " + e.note + " ($" + text(e.amount) + ")"]))),
    el("p", { class: "total" }, ["total: $" + text(total())]),
  ])
}

// A plain form POST: record the entry, then redirect back to the board.
// Transport-invisible (§9.2) — a browser posts a form, a client posts
// JSON, and the same handler serves both.
part add {
  route = "/add"
  handle pipe = (req: std.Request) => {
    record(text(req.data.who), text(req.data.note), number(text(req.data.amount)) ?? 0)
    return redirect("/")
  }
}
