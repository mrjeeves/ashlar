space foundry

part app {
  port = 8080
  style = "foundry"
}

// One brief, on its way through. A data shape: fields only (§5).
part Job {
  id: text
  brief: text
  at: number
}

// One named queue is the joint between the API, background worker, and
// live interface. `accept` returns while the work is still waiting;
// `spawn` runs `finish` between requests, and reactive reads push the
// completed result into every connected board.
//
// `held` is what makes the queue visible: hold the line and briefs pile up
// where everyone watching can see them — and anybody can release it, because
// the state is the program's, not the page's (§9.3).
part Queue {
  state waiting: [Job] = []
  state finished: [Job] = []
  state held: bool = false

  accept = (brief: text) => {
    waiting = [...waiting, { id: id(), brief: brief, at: now() }]
    spawn(() => finish())
  }
  finish = () => {
    if held {
      return
    }
    let next = waiting[0]
    if next != none {
      waiting = slice(waiting, 1, len(waiting))
      finished = slice([next!, ...finished], 0, 8)
    }
  }
  hold = () => {
    held = true
  }
  // Locals are single-assignment and there is no `while`, so draining is
  // recursion — guarded by the queue's own length (§7).
  release = () => {
    held = false
    drain()
  }
  drain = () => {
    if len(waiting) > 0 {
      finish()
      drain()
    }
  }
  cancel = (which: text) => {
    waiting = filter(waiting, (j: Job) => j.id != which)
  }
}

part submit {
  route = "/api/jobs"
  handle pipe = (req: std.Request) => {
    let d = fields(req.data) ?? fail(400, "send a JSON object: { brief }")
    let brief = text(d["brief"] ?? "")
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
  view = () => el("div", { class: "stage" }, [
    el("title", {}, ["foundry"]),
    el(board, {}),
  ])
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
      el("input", { class: "field", oninput: typed, value: draft, placeholder: "a brief to run", autocomplete: "off" }, []),
      el("button", { class: "primary" }, ["queue"]),
    ]),
    el("div", { class: "lanes" }, [
      lane("waiting", Queue.waiting, true),
      lane("finished", Queue.finished, false),
    ]),
    el("footer", { class: "bar" }, [
      el("span", { class: "count" }, ["waiting: " + text(len(Queue.waiting))]),
      el("button", { class: gate_class(), onclick: gate }, [gate_label()]),
    ]),
  ])

  lane = (name: text, jobs: [Job], live: bool) => el("div", { class: "lane" }, [
    el("p", { class: "lanetitle" }, [name]),
    el("ul", { class: "joblist" }, cards(jobs, live)),
  ])

  cards = (jobs: [Job], live: bool) => {
    if len(jobs) == 0 {
      return [el("li", { class: "jobnone" }, ["nothing"])]
    }
    return map(jobs, (j: Job) => el("li", { class: "job" }, [
      el("span", { class: "jobname" }, [j.brief]),
    ] + scrap(j, live)))
  }
  // Only something still waiting can be called off.
  scrap = (j: Job, live: bool) => (if live { [el("button", { class: "drop", title: "cancel", onclick: (e: std.Event) => Queue.cancel(j.id) }, ["×"])] } else { [] })

  gate = () => {
    if Queue.held {
      Queue.release()
    } else {
      Queue.hold()
    }
  }
  gate_label = () => (if Queue.held { "release the line" } else { "hold the line" })
  gate_class = () => (if Queue.held { "gate holding" } else { "gate" })

  typed = (e: std.Event) => {
    draft = text(e.data.value)
  }
  queue = () => {
    if draft != "" {
      Queue.accept(draft)
    }
    draft = ""
  }
}
