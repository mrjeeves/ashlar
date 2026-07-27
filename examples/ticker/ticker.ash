space ticker

part app {
  port = 8080
  style = "ticker"
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

// The directory form of `files` (§9.8): the whole folder under this route,
// as against the single-file form the root paths use. Both forms are in the
// corpus so both are defended.
part docs {
  route = "/docs"
  files = "docs"
}

// A schedule reported in the units a human writes: minutes, not 60000ms.
// `log.warn` and `log.debug` exist alongside `log.info`; a corpus that only
// ever calls one is a corpus that would not notice the others breaking.
part Sweep {
  state swept: number = 0
  every = "5m"
  run = () => {
    swept = swept + 1
    log.debug("sweep", { n: swept })
    if swept > 100 {
      log.warn("sweeping a great deal", { n: swept })
    }
  }
}

part page {
  route = "/"
  view = () => el("div", { class: "stage" }, [
    el("div", { class: "card" }, [
      el("p", { class: "kicker" }, ["schedule · §9.7"]),
      el("h1", {}, ["ticker"]),
      el("p", { class: "lede" }, ["A server-side schedule bumps a counter five times a second. No browser code, no polling — the page just re-renders."]),
      el(face, {}),
    ]),
  ])
}

part face {
  view = () => el("div", { class: "beat" }, [
    el("span", { class: "num" }, [text(ticker.Clock.beats)]),
    el("span", { class: "unit" }, ["beats"]),
  ])
}

part api {
  route = "/api/beats"
  handle pipe = (req: std.Request) => ticker.Clock.beats
}
