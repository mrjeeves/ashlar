space foundry

part app {
  port = 8080
}

// One named queue is the joint between the API, background worker, and
// live interface. `accept` returns while the work is still waiting;
// `spawn` runs `finish` between requests, and reactive reads push the
// completed result into every connected board.
part Queue {
  state waiting: [text] = []
  synced finished: [text] = []
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
  view = () => el(board, {})
}

part board {
  view = () => el("main", {}, [
    el("h2", {}, ["agent foundry"]),
    el("p", {}, ["waiting: " + text(len(Queue.waiting))]),
    el("p", {}, ["finished: " + join(Queue.finished, ", ")]),
  ])
}
