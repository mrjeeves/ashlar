space diary

part app {
  port = 8080
}

part register {
  route = "/api/signup"
  handle pipe = (req: std.Request) => signup(text(req.data.email), text(req.data.password))
}

part session {
  route = "/api/login"
  handle pipe = (req: std.Request) => login(text(req.data.email), text(req.data.password))
}

part quit {
  route = "/api/logout"
  handle pipe = (req: std.Request) => {
    logout()
    return "bye"
  }
}

// `allow` runs before `handle`; false is a 403 (§9.6). Inside, the
// session is proven, so `req.user!` cannot fault.
part private {
  route = "/private"
  allow = (req: std.Request) => req.user != none
  handle pipe = (req: std.Request) => "dear diary, from " + req.user!.email
}
