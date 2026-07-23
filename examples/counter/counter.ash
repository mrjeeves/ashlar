space counter

part app {
  port = 8080
}

// Views render on the server; the browser runs a transport shim and no
// program code. el(tally, {...}) instantiates the part below and sets
// its `label` prop (§9.4).
part page {
  route = "/"
  view = () => el(tally, { label: "clicks" })
}

// `label` is a prop the caller sets; `state n` belongs to each
// instance. Clicking runs `bump` on the server, and every view that
// read `n` re-renders and patches in place — on every client.
part tally {
  label: text
  state n: number = 0
  view = () => el("button", { onclick: bump }, [label + ": " + text(n)])
  bump = () => {
    n = n + 1
  }
}
