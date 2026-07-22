space chat.widgets

part counter {
  label: text
  state n: number = 0
  view = () => el("button", { onclick: bump }, [label + ": " + text(n)])
  bump = () => { n = n + 1 }
}
