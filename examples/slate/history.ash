space slate.history
use slate.limits

// Undo, which on a shared pad cannot mean "undo my last keystroke" —
// there is no my. It means "put the pad back the way it was at a moment
// everyone can point at", so this space snapshots the text on a rhythm
// and the editor lists those moments.
//
// It layers the same seam slate.limits does, and its `use` of that space
// is what orders them: a refused edit must not be snapshotted, so the
// size check has to have run first (§3). Swap the `use` and the order
// swaps with it — that is the whole ordering mechanism.

part Keeping {
  setting everyEdits: number = 12
}

part slate.data.Store {
  tags append = ["history"]

  apply pipe = (e: slate.data.Edit) => {
    if e.refused == "" {
      let p = pads[e.pad]
      if p != none and p!.edits > 0 and p!.edits % slate.history.Keeping.everyEdits == 0 {
        snapshot(e.pad, e.who)
      }
    }
    return e
  }
}

// A pad nobody has touched for a while gets one final snapshot, so the
// last state of an abandoned pad is recoverable even though no edit will
// ever cross the threshold above (§9.7).
part sweep {
  every = "30s"
  state swept: number = 0
  run = () => {
    swept = swept + 1
    for key in keys(slate.data.Store.pads) {
      let p = slate.data.Store.pads[key]!
      if p.body != "" and now() - p.touched > 30000 and quiet(key, p.body) {
        slate.data.Store.snapshot(key, "the sweep")
      }
    }
  }

  // Only if the last thing kept is not already this text: a sweep that
  // ran every thirty seconds forever would otherwise fill the list with
  // identical copies of an untouched pad.
  quiet = (key: text, body: text) => {
    let kept = slate.data.Store.revisionsOn(key)
    let last = kept[0]
    return if last != none { last!.body != body } else { true }
  }
}
