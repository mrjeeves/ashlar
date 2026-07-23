space chat.audit
use chat.api
use chat.data

// The signature move: extend parts that live in other files, from your
// own space, without editing theirs. This `prepare` layer logs and
// passes the body through; chat.data's 200-char clamp still runs —
// pipe layers chain, they never replace (§2, §4).
part chat.data.Store {
  prepare pipe = (body: text) => {
    log.info("prepared", { size: len(body) })
    return body
  }
}

// A second `start` layer for the same app part: stacks run every
// layer on boot, so the audit space announces itself alongside.
part chat.api.app {
  start stack = () => {
    log.info("audit online")
    return none
  }
}
