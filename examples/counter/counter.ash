space counter

part app {
  port = 8080
}

part page {
  route = "/"
  view = () => el(tally, { label: "clicks" })
}

part tally {
  label: text
  state n: number = 0
  view = () => el("button", { onclick: bump }, [label + ": " + text(n)])
  bump = () => {
    n = n + 1
  }
}
