space chat.notify

part alerts {
  port = 8080
  start stack = () => {
    subscribe("alerts", (msg: data) => log.info("alert", { msg: msg }))
    return none
  }
  raise = (body: text) => {
    publish("alerts", { body: body })
  }
}
