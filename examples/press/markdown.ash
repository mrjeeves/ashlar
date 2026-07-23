space press.markdown
use press

// A second space thickens the pipeline without editing base.ash:
// `append` joins, `deep` merges one level, `pipe` chains base-first,
// `stack reverse` tears down derived-first (§4).
part press.Pipeline {
  tags append = ["markdown"]
  limits deep = { depth: 3 }
  render pipe = (t: text) => "<p>" + t + "</p>"
  boot stack = () => {
    log.info("press: markdown online")
    return none
  }
  halt stack reverse = () => {
    log.info("press: markdown down")
    return none
  }
}
