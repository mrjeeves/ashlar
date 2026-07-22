space chat.api

part messages {
  route = "/api/messages"
  handle pipe = (req: std.Request) => req
}

// file: b.ash
space chat.api.logging
use chat.api

part chat.api.messages {
  handle pipe = (req: std.Request) => {
    log.info("handled", { path: req.path })
    return req
  }
}
