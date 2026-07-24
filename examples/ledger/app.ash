space ledger
use ledger.store

part app {
  port = 8080
  style = "ledger"
}

// The board renders the whole ledger from SQLite on every request: the
// entries newest-first, and the running total, which SQL sums in the shim
// (ledger.store.total) — Ashlar composes, the database aggregates. Because
// `recent`/`total` are `reads Entry` (data.ash), the view depends on the
// Entry collection; the moment anything `writes Entry`, this board
// re-renders and patches — live, over SQL, across every open window.
part board {
  route = "/"
  state who: text = ""
  state note: text = ""
  state amount: text = ""
  view = () => el("main", { class: "ledger" }, [
    el("h1", {}, ["Ledger"]),
    el("form", { class: "add", onsubmit: save }, [
      el("input", { oninput: setWho, value: who, placeholder: "who" }, []),
      el("input", { oninput: setNote, value: note, placeholder: "for what" }, []),
      el("input", { oninput: setAmount, value: amount, placeholder: "amount" }, []),
      el("button", {}, ["record"]),
    ]),
    el("ul", { class: "rows" }, map(recent(), (e: Entry) => el("li", { class: "row" }, [e.who + ": " + e.note + " ($" + text(e.amount) + ")"]))),
    el("p", { class: "total" }, ["total: $" + text(total())]),
  ])
  setWho = (e: std.Event) => {
    who = text(e.data.value)
  }
  setNote = (e: std.Event) => {
    note = text(e.data.value)
  }
  setAmount = (e: std.Event) => {
    amount = text(e.data.value)
  }
  save = () => {
    record(who, note, number(amount) ?? 0)
    who = ""
    note = ""
    amount = ""
  }
}

// The HTTP API a programmatic client uses: record the entry, then redirect
// back to the board. Transport-invisible (§9.2) — JSON here, the socket
// form above, one `record`. Either write patches every connected board,
// because `record` `writes Entry` (§9.3).
part add {
  route = "/add"
  handle pipe = (req: std.Request) => {
    record(text(req.data.who), text(req.data.note), number(text(req.data.amount)) ?? 0)
    return redirect("/")
  }
}
