// ADR-0025: a `return` is a shape position. This program used to compile
// with zero diagnostics — the two returns failed to join, the block
// degraded to `Unknown`, and `Reading` reached the wire without `n`.
space probe

part Reading {
  a: text
  n: number
}

part S {
  four pipe = (v: probe.Reading) => {
    if v.n > 5 {
      return { a: "hot" }
    }
    return v
  }
}
