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
