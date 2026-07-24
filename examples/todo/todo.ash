space todo

part app {
  port = 8080
  style = "todo"
}

part page {
  route = "/"
  view = () => el("div", { class: "stage" }, [el(board, {})])
}

// A live form: `oninput` mirrors the field into per-instance state,
// `onsubmit` commits it. Handlers run server-side; the browser only
// forwards events (§9.4).
part board {
  state items: [text] = []
  state draft: text = ""
  view = () => el("div", { class: "card" }, [
    el("p", { class: "kicker" }, ["live form · §9.4"]),
    el("h1", {}, ["todo"]),
    el("form", { class: "row", onsubmit: add }, [
      el("input", { class: "field", oninput: typed, value: draft, name: "item", placeholder: "add an item" }, []),
      el("button", { class: "primary" }, ["add"]),
    ]),
    el("ul", { class: "list" }, rows()),
  ])
  rows = () => map(items, (entry: text) => el("li", { class: "item" }, [entry]))
  typed = (e: std.Event) => {
    draft = text(e.data.value)
  }
  add = () => {
    items = [...items, draft]
    draft = ""
  }
}
