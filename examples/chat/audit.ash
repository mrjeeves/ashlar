space chat.audit
use chat.api
use chat.data

part chat.data.Store {
  prepare pipe = (body: text) => {
    log.info("prepared", { size: len(body) })
    return body
  }
}

part chat.api.app {
  start stack = () => {
    log.info("audit online")
    return none
  }
}
