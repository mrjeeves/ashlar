space slate.limits
use slate.data

// A pad on the open internet is a text field anyone can paste into, so
// the size cap is not a nicety. This space layers the edit seam and
// refuses what it will not hold — without editing slate.data, and
// without the store knowing a policy exists.

part Policy {
  setting maxChars: number = 20000
  setting maxLines: number = 600
}

part slate.data.Store {
  tags append = ["limits"]

  apply pipe = (e: slate.data.Edit) => {
    if len(e.incoming) > slate.limits.Policy.maxChars {
      return holdEdit({
        ...e,
        refused: "a pad holds " + text(slate.limits.Policy.maxChars) + " characters; that edit is " + text(len(e.incoming)),
      })
    }
    if len(split(e.incoming, "\n")) > slate.limits.Policy.maxLines {
      return holdEdit({
        ...e,
        refused: "a pad holds " + text(slate.limits.Policy.maxLines) + " lines",
      })
    }
    return e
  }
}
