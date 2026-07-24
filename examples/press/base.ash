space press

// One part, four merge kinds. Each layer that touches a property must
// restate its kind (C5) — the identity is part of the name's meaning.
part Pipeline {
  tags append: [text] = ["core"]
  limits deep: {text: number} = { size: 100 }
  render pipe = (t: text) => t
  boot stack = () => {
    log.info("press: base online")
    return none
  }
  halt stack reverse = () => {
    log.info("press: base down")
    return none
  }
}

part app {
  port = 8080
  style = "press"
}

// A live window onto the composed pipe: whatever you type runs through
// `render` — base first, then the markdown layer (§4) — and the output
// updates as you type. No route round-trip; the handler runs over the
// socket (§9.4).
part page {
  route = "/"
  view = () => el("div", { class: "stage" }, [el(studio, {})])
}

part studio {
  state draft: text = "hello"
  view = () => el("div", { class: "card" }, [
    el("p", { class: "kicker" }, ["layered pipe · §4"]),
    el("h1", {}, ["press"]),
    el("p", { class: "lede" }, ["Your text runs through the composed render pipe — base first, then the markdown layer — and the output updates as you type."]),
    el("input", { class: "field", oninput: typed, value: draft, placeholder: "type something" }, []),
    el("p", { class: "outlabel" }, ["rendered output"]),
    el("pre", { class: "out" }, [Pipeline.render(draft)]),
  ])
  typed = (e: std.Event) => {
    draft = text(e.data.value)
  }
}

part config {
  route = "/api/config"
  handle pipe = (req: std.Request) => {
    return { tags: Pipeline.tags, limits: Pipeline.limits }
  }
}

part render {
  route = "/api/render"
  handle pipe = (req: std.Request) => Pipeline.render(text(req.data.body))
}
