space locker

part app {
  port = 8080
  style = "locker"
}

// A personal locker. `owned stored notes` gives every signed-in user their
// OWN list, saved to disk and isolated from everyone else's — no keying by
// user id, and no way to reach another user's (ADR-0015). `owned` has no
// meaning without a user, so every reader below runs behind a signed-in
// session; reaching it anonymously would fault (§9.3).
part Store {
  owned stored notes: [text] = []
  keep = (note: text) => {
    notes = [...notes, note]
  }
}

// The front page renders a view, never a redirect: a signed-in user gets
// their board, everyone else meets the gate. The board is only built when
// a user is present, so its `owned` reads always resolve (§9.3).
part home {
  route = "/"
  handle pipe = (req: std.Request) => {
    return if req.user != none { el(board, {}) } else { el(gate, {}) }
  }
}

// The board keeps notes over the socket and reads `Store.notes`, which
// resolves to THIS user's list — the instance captured its owner when the
// page mounted, so every re-render stays on the same locker.
part board {
  state draft: text = ""
  view = () => el("div", { class: "stage" }, [
    el("div", { class: "card" }, [
      el("p", { class: "kicker" }, ["per-user storage · ADR-0015"]),
      el("h1", {}, ["locker"]),
      el("p", { class: "lede" }, ["Every signed-in person gets their own notes — owned storage, isolated by construction and saved to disk."]),
      el("form", { class: "row", onsubmit: keep }, [
        el("input", { class: "field", oninput: typed, value: draft, placeholder: "keep a note" }, []),
        el("button", { class: "primary" }, ["keep"]),
      ]),
      el("ul", { class: "list" }, rows()),
      el("a", { class: "ghost", href: "/leave" }, ["log out"]),
    ]),
  ])
  rows = () => map(Store.notes, (note: text) => el("li", { class: "item" }, [note]))
  typed = (e: std.Event) => {
    draft = text(e.data.value)
  }
  keep = () => {
    Store.keep(draft)
    draft = ""
  }
}

part gate {
  view = () => el("div", { class: "stage" }, [
    el("div", { class: "card" }, [
      el("p", { class: "kicker" }, ["per-user storage · ADR-0015"]),
      el("h1", {}, ["locker"]),
      el("p", { class: "lede" }, ["Sign in and your notes are yours alone — no one else can reach them."]),
      el("form", { class: "stack", action: "/join", method: "post" }, [
        el("h2", {}, ["create an account"]),
        el("input", { class: "field", name: "email", type: "email", placeholder: "you@example.com" }, []),
        el("input", { class: "field", name: "password", type: "password", placeholder: "password" }, []),
        el("button", { class: "primary" }, ["sign up"]),
      ]),
      el("form", { class: "stack", action: "/enter", method: "post" }, [
        el("h2", {}, ["or log in"]),
        el("input", { class: "field", name: "email", type: "email", placeholder: "you@example.com" }, []),
        el("input", { class: "field", name: "password", type: "password", placeholder: "password" }, []),
        el("button", { class: "ghost" }, ["log in"]),
      ]),
    ]),
  ])
}

// Browser-facing auth: run the builtin, then redirect home so the board
// renders with the new session in place.
part join {
  route = "/join"
  handle pipe = (req: std.Request) => {
    signup(text(req.data.email), text(req.data.password))
    return redirect("/")
  }
}

part enter {
  route = "/enter"
  handle pipe = (req: std.Request) => {
    login(text(req.data.email), text(req.data.password))
    return redirect("/")
  }
}

part leave {
  route = "/leave"
  handle pipe = (req: std.Request) => {
    logout()
    return redirect("/")
  }
}

// The API surface (§9.2) a programmatic client uses: accounts, this user's
// notes, and a keep. Each read guards with `allow`, rejecting anonymous
// callers before the owned read runs.
part register {
  route = "/api/signup"
  handle pipe = (req: std.Request) => signup(text(req.data.email), text(req.data.password))
}

part session {
  route = "/api/login"
  handle pipe = (req: std.Request) => login(text(req.data.email), text(req.data.password))
}

part list {
  route = "/api/notes"
  allow = (req: std.Request) => req.user != none
  handle pipe = (req: std.Request) => Store.notes
}

part add {
  route = "/api/keep"
  allow = (req: std.Request) => req.user != none
  handle pipe = (req: std.Request) => {
    Store.keep(text(req.data.note))
    return "ok"
  }
}
