space hello

// A part with `port` is the server (§9.1). No main, no router, no
// wiring — names do all of it, and the build computes the rest.
part app {
  port = 8080
  style = "hello"
}

// A part with `route` is an endpoint; returning text answers the
// request. `handle` is a pipe so other spaces can layer on it (§9.2).
part greet {
  route = "/text"
  handle pipe = (req: std.Request) => "hello from ashlar"
}

// How many people have this page open. `state` on a singleton is one value
// for the whole program (§9.3), and a view's own lifecycle is the entire
// bookkeeping: `start` runs when a page mounts, `stop` when its socket goes
// (§9.4). Nothing polls, nothing heartbeats, nothing can get out of step.
part Room {
  state here: number = 0
  came = () => {
    here = here + 1
  }
  went = () => {
    here = here - 1
  }
}

part page {
  route = "/"
  view = () => el(hail, {})
}

part hail {
  start stack = () => {
    Room.came()
    return none
  }
  stop stack reverse = () => {
    Room.went()
    return none
  }
  view = () => el("div", { class: "stage" }, [
    el("title", {}, ["hello"]),
    el("h1", {}, ["hello from ashlar"]),
    el("p", { class: "lede" }, [company()]),
    el("p", { class: "foot" }, ["Five parts. The same server also answers /text as plain text."]),
  ])
  // Every page read `here`, so every page re-renders when it moves — which
  // is what makes opening a second window visible in the first.
  company = () => (if Room.here < 2 { "You are the only one here." } else { text(Room.here) + " of you have this open right now." })
}
