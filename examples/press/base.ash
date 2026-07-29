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
    el("title", {}, ["press"]),
    el("p", { class: "kicker" }, ["layered pipe · §4"]),
    el("h1", {}, ["press"]),
    el("p", { class: "lede" }, ["One part, four merge kinds, two spaces. Everything below is the COMPOSED part — nothing on this page reads a layer."]),
    el("input", { class: "field", oninput: typed, value: draft, placeholder: "type something" }, []),
    el("div", { class: "flow" }, [
      el("div", { class: "step" }, [
        el("p", { class: "steplabel" }, ["in"]),
        el("pre", { class: "out" }, [draft]),
      ]),
      el("span", { class: "arrow" }, ["→"]),
      el("div", { class: "step" }, [
        el("p", { class: "steplabel" }, ["through render pipe"]),
        el("pre", { class: "out" }, [Pipeline.render(draft)]),
      ]),
    ]),
    el("div", { class: "kinds" }, [
      kind("append", "tags", join(Pipeline.tags, " + ")),
      kind("deep", "limits", shown()),
      kind("pipe", "render", text(len(Pipeline.tags)) + " layers, base first"),
      kind("stack", "boot / halt", "up in use order, down in reverse"),
    ]),
  ])
  // The merged map, read back out. `deep` put `depth` beside `size` without
  // either space knowing about the other.
  shown = () => join(map(keys(Pipeline.limits), (k: text) => k + " " + text(Pipeline.limits[k] ?? 0)), ", ")
  kind = (name: text, prop: text, value: text) => el("div", { class: "kind" }, [
    el("span", { class: "kname" }, [name]),
    el("span", { class: "kprop" }, [prop]),
    el("span", { class: "kvalue" }, [value]),
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
  handle pipe = (req: std.Request) => {
    let d = fields(req.data) ?? fail(400, "send a JSON object: { body }")
    return Pipeline.render(text(d["body"] ?? ""))
  }
}
