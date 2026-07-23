space todo

part app {
  port = 8080
}

part page {
  route = "/"
  view = () => el(board, {})
}

// A live form: `oninput` mirrors the field into per-instance state,
// `onsubmit` commits it. Handlers run server-side; the browser only
// forwards events (§9.4).
part board {
  state items: [text] = []
  state draft: text = ""
  view = () => el("div", {}, [
    el("form", { onsubmit: add }, [
      el("input", { oninput: typed, value: draft, name: "item" }, []),
      el("button", {}, ["add"]),
    ]),
    el("p", {}, ["todo: " + join(items, ", ")]),
  ])
  typed = (e: std.Event) => {
    draft = text(e.data.value)
  }
  add = () => {
    items = [...items, draft]
    draft = ""
  }
}
