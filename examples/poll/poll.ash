space poll

part app {
  port = 8080
  style = "poll"
}

// Votes are stored state: reactivity alone keeps every tally live (§9.3).
// The channel carries what state doesn't — the ephemeral event itself.
//
// The ballot is stored too, so anybody on the page can put a stone on it and
// everyone else's page grows the row. Nothing here is seeded in a view: the
// `start` stack fills an empty store once, so a fresh install has something
// to vote on and a used one is left alone.
part Store {
  stored options: [text] = []
  stored votes: {text: number} = {}
  cast = (option: text) => {
    votes = put(votes, option, (votes[option] ?? 0) + 1)
    publish("poll.activity", option)
  }
  propose = (option: text) => {
    if not contains(options, option) {
      options = [...options, option]
      publish("poll.activity", option + " is on the ballot")
    }
  }
  seed = () => {
    if len(options) == 0 {
      options = ["granite", "marble", "slate"]
    }
  }
  // Locals are single-assignment, so a running total is recursion rather
  // than a mutable accumulator — named functions may call themselves (§7).
  total = () => added(map(options, (o: text) => Store.votes[o] ?? 0), 0)
  added = (xs: [number], i: number) => (if i >= len(xs) { 0 } else { xs[i]! + added(xs, i + 1) })
}

part page {
  route = "/"
  start stack = () => {
    Store.seed()
    return none
  }
  view = () => el("div", { class: "stage" }, [
    el("title", {}, ["poll"]),
    el(board, {}),
  ])
}

// Each board instance subscribes in its start stack (§9.5): the
// subscription lives with the instance and ends when it unmounts.
// `latest` is per-instance — a fresh page starts at "none yet" no
// matter how many votes came before it.
part board {
  state latest: text = "none yet"
  state draft: text = ""
  start stack = () => {
    subscribe("poll.activity", note)
    return none
  }
  note = (m: data) => {
    latest = text(m)
  }
  view = () => el("div", { class: "card" }, [
    el("p", { class: "kicker" }, ["channel · §9.5"]),
    el("h1", {}, ["which stone?"]),
    el("div", { class: "choices" }, buttons()),
    el("form", { class: "add", onsubmit: put_up }, [
      el("input", { class: "field", oninput: typed, value: draft, name: "stone", placeholder: "put another stone on the ballot", autocomplete: "off" }, []),
      el("button", { class: "quiet" }, ["add"]),
    ]),
    el("p", { class: "tally" }, ["tally: " + summary()]),
    el("p", { class: "latest" }, ["last vote: " + latest]),
  ])
  buttons = () => map(Store.options, (o: text) => el(choice, { option: o, share: share(o) }))
  // A bar's width is the number, not the appearance of it — the same reason
  // pong places its ball with inline geometry.
  share = (o: text) => (if Store.total() == 0 { 0 } else { (Store.votes[o] ?? 0) * 100 / Store.total() })
  summary = () => join(map(Store.options, (o: text) => o + " " + text(Store.votes[o] ?? 0)), " / ")
  typed = (e: std.Event) => {
    draft = text(e.data.value ?? "")
  }
  put_up = () => {
    if draft != "" {
      Store.propose(draft)
    }
    draft = ""
  }
}

part choice {
  option: text
  share: number
  view = () => el("button", { class: "vote", onclick: pick }, [
    el("span", { class: "fill", style: "width:" + text(share) + "%" }, []),
    el("span", { class: "name" }, [option]),
    el("span", { class: "n" }, [text(poll.Store.votes[option] ?? 0)]),
  ])
  pick = () => {
    Store.cast(option)
  }
}

part results {
  route = "/api/votes"
  handle pipe = (req: std.Request) => Store.votes
}

part ballot {
  route = "/api/vote"
  handle pipe = (req: std.Request) => {
    let d = fields(req.data) ?? fail(400, "send a JSON object: { option }")
    Store.cast(text(d["option"] ?? ""))
    return "ok"
  }
}
