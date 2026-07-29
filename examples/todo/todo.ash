space todo

part app {
  port = 8080
  style = "todo"
}

// One item. Fields only, so it is a data shape and its values are written
// as plain literals (§5).
part Item {
  id: text
  what: text
  done: bool = false
}

// The list everybody shares. `stored` puts it on disk, so it survives a
// restart (§9.3); on a singleton — a part nothing instantiates — it is ONE
// list for every window, and every view that read it re-renders when it
// moves. Two browsers on this page are working on the same list.
part List {
  stored items: [Item] = []
  state here: number = 0

  add = (what: text) => {
    items = [...items, { id: id(), what: what, done: false }]
  }
  // Values are immutable, so changing one item means a new list with a new
  // item in its place (§9.3) — a spread copies the rest.
  toggle = (which: text) => {
    items = map(items, (it: Item) => flipped(it, which))
  }
  flipped = (it: Item, which: text) => {
    if it.id != which {
      return it
    }
    return { ...it, done: not it.done }
  }
  forget = (which: text) => {
    items = filter(items, (it: Item) => it.id != which)
  }
  sweep = () => {
    items = filter(items, (it: Item) => not it.done)
  }
  left = () => len(filter(items, (it: Item) => not it.done))

  // Presence by lifecycle (§9.4): a page mounting arrives, its socket
  // closing departs. No heartbeat, and nothing to get out of step.
  came = () => {
    here = here + 1
  }
  went = () => {
    here = here - 1
  }
}

part page {
  route = "/"
  view = () => el("div", { class: "stage" }, [
    el("title", {}, ["todo"]),
    el(board, {}),
  ])
}

// A live form: `oninput` mirrors the field into per-instance state,
// `onsubmit` commits it. Handlers run server-side; the browser only
// forwards events (§9.4).
part board {
  state draft: text = ""

  start stack = () => {
    List.came()
    return none
  }
  stop stack reverse = () => {
    List.went()
    return none
  }

  view = () => el("div", { class: "card" }, [
    el("header", { class: "top" }, [
      el("h1", {}, ["todo"]),
      el("span", { class: "who" }, [watching()]),
    ]),
    el("p", { class: "lede" }, ["One list, shared by every window on it. Kept on disk, so it is still here after a restart."]),
    el("form", { class: "row", onsubmit: add }, [
      el("input", { class: "field", oninput: typed, value: draft, name: "item", placeholder: "add an item", autocomplete: "off" }, []),
      el("button", { class: "primary" }, ["add"]),
    ]),
    el("ul", { class: "list" }, rows()),
    el("footer", { class: "foot" }, [
      el("span", {}, [remaining()]),
    ] + sweeper()),
  ])

  watching = () => (if List.here < 2 { "just you" } else { text(List.here) + " here" })
  remaining = () => (if len(List.items) == 0 { "nothing yet" } else { text(List.left()) + " of " + text(len(List.items)) + " to go" })
  sweeper = () => (if List.left() == len(List.items) { [] } else { [el("button", { class: "sweep", onclick: clear }, ["clear done"])] })
  clear = () => {
    List.sweep()
  }

  rows = () => map(List.items, (it: Item) => el("li", { class: if it.done { "item done" } else { "item" } }, [
    el("button", { class: "tick", title: "done", onclick: (e: std.Event) => List.toggle(it.id) }, [if it.done { "✓" } else { "" }]),
    el("span", { class: "what" }, [it.what]),
    el("button", { class: "drop", title: "remove", onclick: (e: std.Event) => List.forget(it.id) }, ["×"]),
  ]))

  typed = (e: std.Event) => {
    draft = text(e.data.value ?? "")
  }
  add = () => {
    if draft != "" {
      List.add(draft)
    }
    draft = ""
  }
}
