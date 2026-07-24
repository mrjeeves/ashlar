space foundry

part app {
  port = 8080
  style = "foundry"
}

// One named queue is the joint between the API, background worker, and
// live interface. `accept` returns while the work is still waiting;
// `spawn` runs `finish` between requests, and reactive reads push the
// completed result into every connected board.
part Queue {
  state waiting: [text] = []
  state finished: [text] = []
  accept = (brief: text) => {
    waiting = [...waiting, brief]
    spawn(() => finish())
  }
  finish = () => {
    let next = waiting[0]
    if next != none {
      waiting = slice(waiting, 1, len(waiting))
      finished = [...finished, next!]
    }
  }
}

part submit {
  route = "/api/jobs"
  handle pipe = (req: std.Request) => {
    let brief = text(req.data.brief)
    Queue.accept(brief)
    return { accepted: brief }
  }
}

part status {
  route = "/api/status"
  handle pipe = (req: std.Request) => {
    return { waiting: Queue.waiting, finished: Queue.finished }
  }
}

part page {
  route = "/"
  view = () => el("div", { class: "stage" }, [el(board, {})])
}

// The board queues work over the socket and reads the shared queue, so a
// brief submitted here — or over the HTTP API — patches every open board
// the moment the worker finishes it.
part board {
  state draft: text = ""
  view = () => el("div", { class: "card" }, [
    el("p", { class: "kicker" }, ["background work · §9.7"]),
    el("h1", {}, ["agent foundry"]),
    el("p", { class: "lede" }, ["Queue a brief and it returns at once; a worker runs it between requests and pushes the result to every open board."]),
    el("form", { class: "row", onsubmit: queue }, [
      el("input", { class: "field", oninput: typed, value: draft, placeholder: "a brief to run" }, []),
      el("button", { class: "primary" }, ["queue"]),
    ]),
    el("div", { class: "stats" }, [
      el("p", { class: "stat" }, ["waiting: " + text(len(Queue.waiting))]),
      el("p", { class: "stat done" }, ["finished: " + join(Queue.finished, ", ")]),
    ]),
  ])
  typed = (e: std.Event) => {
    draft = text(e.data.value)
  }
  queue = () => {
    Queue.accept(draft)
    draft = ""
  }
}
