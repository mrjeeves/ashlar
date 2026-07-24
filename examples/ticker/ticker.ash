space ticker

part app {
  port = 8080
}

// A scheduled part: the runtime calls `run` on the `every` interval
// (§9.7). The bump is pushed to every connected view that read it —
// server-driven reactivity, no user event anywhere.
part Clock {
  state beats: number = 0
  every = "200ms"
  run = () => {
    beats = beats + 1
  }
}

part page {
  route = "/"
  view = () => el(face, {})
}

part face {
  view = () => el("span", {}, ["beats: " + text(ticker.Clock.beats)])
}

part api {
  route = "/api/beats"
  handle pipe = (req: std.Request) => ticker.Clock.beats
}
