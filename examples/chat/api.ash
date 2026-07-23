space chat.api
use chat.data

part app {
  port = 8080
  start stack = () => {
    log.info("chat is up")
    return none
  }
}

part list {
  route = "/api/messages"
  handle pipe = (req: std.Request) => Store.messages
}

part post {
  route = "/api/post"
  handle pipe = (req: std.Request) => {
    Store.add({
      id: id(),
      author: text(req.data.author),
      body: Store.prepare(text(req.data.body)),
      sent: now(),
    })
    return "ok"
  }
}

part register {
  route = "/api/signup"
  handle pipe = (req: std.Request) => signup(text(req.data.email), text(req.data.password))
}

part session {
  route = "/api/login"
  handle pipe = (req: std.Request) => login(text(req.data.email), text(req.data.password))
}
