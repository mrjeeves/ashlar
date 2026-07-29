space ticker

part app {
  port = 8080
  style = "ticker"
}

// A scheduled part: the runtime calls `run` on the `every` interval
// (§9.7). The bump is pushed to every connected view that read it —
// server-driven reactivity, no user event anywhere.
//
// `laps` is the same shared `state`, written by a person instead of the
// schedule: a click here marks the beat for EVERY page, because a singleton's
// state is one value and every view that read it re-renders (§9.3). One
// property, two writers, and the views cannot tell them apart.
part Clock {
  state beats: number = 0
  state laps: [number] = []
  every = "200ms"
  run = () => {
    beats = beats + 1
  }
  mark = () => {
    laps = slice([beats, ...laps], 0, 8)
  }
  wipe = () => {
    laps = []
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
    el("title", {}, ["ticker"]),
    el("div", { class: "card" }, [
      el("p", { class: "kicker" }, ["schedule · §9.7"]),
      el("h1", {}, ["ticker"]),
      el("p", { class: "lede" }, ["A server-side schedule bumps a counter five times a second. No browser code, no polling — the page just re-renders."]),
      el(face, {}),
      el(marker, {}),
    ]),
  ])
}

part face {
  view = () => el("div", { class: "beat" }, [
    el("span", { class: "pulse" }, []),
    el("span", { class: "num" }, [text(ticker.Clock.beats)]),
    el("span", { class: "unit" }, ["beats"]),
  ])
}

// Mark a beat and everybody watching sees the same mark appear. It is the
// property the schedule writes, written by hand — which is the point.
part marker {
  view = () => el("div", { class: "laps" }, [
    el("div", { class: "lap-row" }, [
      el("button", { class: "mark", onclick: hit }, ["mark this beat"]),
    ] + wiper()),
    el("div", { class: "marks" }, marks()),
  ])
  hit = () => {
    ticker.Clock.mark()
  }
  wipe = () => {
    ticker.Clock.wipe()
  }
  wiper = () => (if len(ticker.Clock.laps) == 0 { [] } else { [el("button", { class: "wipe", onclick: wipe }, ["clear"])] })
  marks = () => {
    if len(ticker.Clock.laps) == 0 {
      return [el("span", { class: "none" }, ["No marks yet. Whoever presses it, everybody sees it."])]
    }
    return map(ticker.Clock.laps, (b: number) => el("span", { class: "mark-chip" }, [text(b)]))
  }
}

part api {
  route = "/api/beats"
  handle pipe = (req: std.Request) => ticker.Clock.beats
}
