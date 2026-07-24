space locker

part app {
  port = 8080
}

// A personal locker. `owned stored notes` gives every signed-in user their
// OWN list, saved to disk and isolated from everyone else's — no keying by
// user id, and no way to reach another user's (ADR-0015). `owned` has no
// meaning without a user, so the routes below guard with `allow`; reaching
// it anonymously would fault (§9.3).
part Store {
  owned stored notes: [text] = []
  keep = (note: text) => {
    notes = [...notes, note]
  }
}

// Accounts (§9.6): signup and login establish the user `owned` scopes by.
part register {
  route = "/api/signup"
  handle pipe = (req: std.Request) => signup(text(req.data.email), text(req.data.password))
}

part session {
  route = "/api/login"
  handle pipe = (req: std.Request) => login(text(req.data.email), text(req.data.password))
}

// This user's notes. The `allow` guard rejects anonymous callers before the
// owned read runs.
part list {
  route = "/api/notes"
  allow = (req: std.Request) => req.user != none
  handle pipe = (req: std.Request) => Store.notes
}

// Keep a note in this user's locker.
part add {
  route = "/api/keep"
  allow = (req: std.Request) => req.user != none
  handle pipe = (req: std.Request) => {
    Store.keep(text(req.data.note))
    return "ok"
  }
}
