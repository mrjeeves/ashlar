space counter

part app {
  port = 8080
  style = "counter"
}

// Views render on the server; the browser runs a transport shim and no
// program code. el(tally, {...}) instantiates the part below and sets
// its `label` prop (§9.4). Appearance is a named boundary: the elements
// carry only `class` names, which meet assets/counter.css by name.
part page {
  route = "/"
  view = () => el("div", { class: "stage" }, [
    el("div", { class: "card" }, [
      el("p", { class: "kicker" }, ["live view · §9.4"]),
      el("h1", {}, ["counter"]),
      el("p", { class: "lede" }, ["Every click runs on the server. The button re-renders in place — on every open window."]),
      el(tally, { label: "clicks" }),
    ]),
  ])
}

// `label` is a prop the caller sets; `state n` belongs to each
// instance. Clicking runs `bump` on the server, and every view that
// read `n` re-renders and patches in place — on every client.
part tally {
  label: text
  state n: number = 0
  view = () => el("button", { class: "count", onclick: bump }, [label + ": " + text(n)])
  bump = () => {
    n = n + 1
  }
}
