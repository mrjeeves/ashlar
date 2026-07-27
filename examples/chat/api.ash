space chat.api
use chat.data

// `use` makes chat.data's bare names visible here (Store, Message).
// `start` is a stack: every layer runs on boot, in `use` order.
part app {
  port = 8080
  style = "chat"
  start stack = () => {
    log.info("chat is up")
    return none
  }
}

// Routes return values; the runtime encodes JSON (§9.2).
part list {
  route = "/api/messages"
  handle pipe = (req: std.Request) => Store.messages
}

// Store.prepare is a pipe call: every layer runs — the clamp in
// chat.data plus whatever other spaces added (chat.audit logs).
part post {
  route = "/api/post"
  handle pipe = (req: std.Request) => {
    let d = fields(req.data) ?? fail(400, "send a JSON object: { author, body }")
    Store.add({
      id: id(),
      author: text(d["author"] ?? ""),
      body: Store.prepare(text(d["body"] ?? "")),
      sent: now(),
    })
    return "ok"
  }
}

// Account builtins: signup/login manage the user store and the
// session cookie themselves (§9.6).
part register {
  route = "/api/signup"
  handle pipe = (req: std.Request) => {
    let d = fields(req.data) ?? fail(400, "send a JSON object: { email, password }")
    return signup(text(d["email"] ?? ""), text(d["password"] ?? ""))
  }
}

part session {
  route = "/api/login"
  handle pipe = (req: std.Request) => {
    let d = fields(req.data) ?? fail(400, "send a JSON object: { email, password }")
    return login(text(d["email"] ?? ""), text(d["password"] ?? ""))
  }
}
