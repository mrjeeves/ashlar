space counter

part app {
  port = 8080
  style = "counter"
}

// Views render on the server; the browser runs a transport shim and no
// program code. Appearance is a named boundary: the elements carry only
// `class` names, which meet assets/counter.css by name.
// Every browser asks for this on every page load. `files` naming one file
// answers one route exactly (§9.8) — a directory here would need `route = "/"`,
// which the page below already has.
part icon {
  route = "/favicon.ico"
  files = "favicon.ico"
}

// The two scopes a `state` property can have, side by side, because the
// difference is the one thing about §9.3 worth learning first and it is
// invisible in a single window.
part page {
  route = "/"
  view = () => el("div", { class: "stage" }, [
    el("title", {}, ["counter"]),
    el("div", { class: "card" }, [
      el("p", { class: "kicker" }, ["live view · §9.4"]),
      el("h1", {}, ["counter"]),
      el("p", { class: "lede" }, ["Two counters, one page. Both run on the server; neither ships a line of code to the browser."]),
      el("div", { class: "pair" }, [
        el(tally, { label: "this window" }),
        el(shared, {}),
      ]),
      el("p", { class: "foot" }, ["Open a second window. The left one counts only where you clicked it. The right one is the same number in both — and it moves in the other window the moment you press it here."]),
    ]),
  ])
}

// `label` is a prop the caller sets; `state n` belongs to each INSTANCE
// (§9.4), so every page gets its own. Clicking runs `bump` on the server,
// and the view that read `n` re-renders and patches in place.
part tally {
  label: text
  state n: number = 0
  view = () => el("button", { class: "count", onclick: bump }, [label + ": " + text(n)])
  bump = () => {
    n = n + 1
  }
}

// The same `state`, on a part nothing instantiates: a singleton is one value
// for the whole program, so every client reads the same number and every
// client re-renders when it moves (§9.3). One keyword's difference.
part Everyone {
  state clicks: number = 0
  hit = () => {
    clicks = clicks + 1
  }
}

part shared {
  view = () => el("button", { class: "count all", onclick: press }, ["everyone: " + text(Everyone.clicks)])
  press = () => {
    Everyone.hit()
  }
}
