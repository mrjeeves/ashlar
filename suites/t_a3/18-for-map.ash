space chat.util

part report {
  state lines: [text] = []
  build = (counts: {text: number}) => {
    lines = []
    for k, v in counts {
      lines = lines + [k + ": " + text(v)]
    }
  }
}
