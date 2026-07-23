space poll

part app {
  port = 8080
}

// Votes are stored state: reactivity alone keeps every tally live (§9.3).
// The channel carries what state doesn't — the ephemeral event itself.
part Store {
  stored votes: {text: number} = {}
  cast = (option: text) => {
    votes = put(votes, option, (votes[option] ?? 0) + 1)
    publish("poll.activity", option)
  }
}

part page {
  route = "/"
  view = () => el(board, {})
}

// Each board instance subscribes in its start stack (§9.5): the
// subscription lives with the instance and ends when it unmounts.
// `latest` is per-instance — a fresh page starts at "none yet" no
// matter how many votes came before it.
part board {
  state latest: text = "none yet"
  options = ["granite", "marble", "slate"]
  start stack = () => {
    subscribe("poll.activity", note)
    return none
  }
  note = (m: data) => {
    latest = text(m)
  }
  view = () => el("div", {}, [
    el("h2", {}, ["which stone?"]),
    el("div", {}, buttons()),
    el("p", {}, ["tally: " + summary()]),
    el("p", {}, ["last vote: " + latest]),
  ])
  buttons = () => map(options, (o: text) => el(choice, { option: o }))
  summary = () => join(map(options, (o: text) => o + " " + text(Store.votes[o] ?? 0)), " / ")
}

part choice {
  option: text
  view = () => el("button", { onclick: pick }, [option])
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
    Store.cast(text(req.data.option))
    return "ok"
  }
}
