space chat.util

part report {
  state lines: [text] = []
  build = (counts: {number}) => {
    lines = []
    for k, v in counts {
      lines = lines + [k + ": " + text(v)]
    }
  }
}
