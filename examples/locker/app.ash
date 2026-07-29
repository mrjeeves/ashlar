space locker

part app {
  port = 8080
  style = "locker"
}

// A personal locker. `peruser stored notes` gives every signed-in user their
// OWN list, saved to disk and isolated from everyone else's — no keying by
// user id, and no way to reach another user's (ADR-0015). `peruser` has no
// meaning without a user, so every reader below runs behind a signed-in
// session; reaching it anonymously would fault (§9.3).
part Store {
  peruser stored notes: [text] = []
  // The same keyword without `peruser`: one list, everybody's. Declared
  // right beside the private one so the difference is a word, not a design.
  stored shelf: [text] = []
  keep = (note: text) => {
    notes = [...notes, note]
  }
  pin = (note: text) => {
    shelf = [...shelf, note]
  }
  // Moving a note to the shelf is the one place the two meet — and it only
  // ever goes this way, because nothing can read another person's locker.
  publish_one = (note: text) => {
    notes = filter(notes, (n: text) => n != note)
    shelf = [...shelf, note]
  }
}

// The front page renders a view, never a redirect: a signed-in user gets
// their board, everyone else meets the gate. The board is only built when
// a user is present, so its `peruser` reads always resolve (§9.3).
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
      el("p", { class: "lede" }, ["Two lists, one keyword apart. The left is yours — per-user storage, isolated by construction. The right is everybody's, and every window on this site sees it move."]),
      el("form", { class: "row", onsubmit: keep }, [
        el("input", { class: "field", oninput: typed, value: draft, placeholder: "keep a note", autocomplete: "off" }, []),
        el("button", { class: "primary" }, ["keep"]),
      ]),
      el("div", { class: "lanes" }, [
        el("div", { class: "lane" }, [
          el("p", { class: "lanetitle mine" }, ["yours alone"]),
          el("ul", { class: "list" }, rows()),
        ]),
        el("div", { class: "lane" }, [
          el("p", { class: "lanetitle" }, ["the shared shelf"]),
          el("ul", { class: "list" }, shared()),
        ]),
      ]),
      el("a", { class: "ghost", href: "/leave" }, ["log out"]),
    ]),
  ])
  // `Store.notes` resolves to THIS user's list; `Store.shelf` is the
  // program's one list. Neither read says which — the declaration did.
  rows = () => {
    if len(Store.notes) == 0 {
      return [el("li", { class: "none" }, ["nothing kept yet"])]
    }
    return map(Store.notes, (note: text) => el("li", { class: "item" }, [
      el("span", { class: "what" }, [note]),
      el("button", { class: "move", title: "put it on the shared shelf", onclick: (e: std.Event) => Store.publish_one(note) }, ["→"]),
    ]))
  }
  shared = () => {
    if len(Store.shelf) == 0 {
      return [el("li", { class: "none" }, ["nothing shared yet"])]
    }
    return map(Store.shelf, (note: text) => el("li", { class: "item" }, [
      el("span", { class: "what" }, [note]),
    ]))
  }
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
    let d = fields(req.data) ?? fail(400, "send a JSON object: { email, password }")
    signup(text(d["email"] ?? ""), text(d["password"] ?? ""))
    return redirect("/")
  }
}

part enter {
  route = "/enter"
  handle pipe = (req: std.Request) => {
    let d = fields(req.data) ?? fail(400, "send a JSON object: { email, password }")
    login(text(d["email"] ?? ""), text(d["password"] ?? ""))
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
// callers before the peruser read runs.
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

part list {
  route = "/api/notes"
  allow = (req: std.Request) => req.user != none
  handle pipe = (req: std.Request) => Store.notes
}

part add {
  route = "/api/keep"
  allow = (req: std.Request) => req.user != none
  handle pipe = (req: std.Request) => {
    let d = fields(req.data) ?? fail(400, "send a JSON object: { note }")
    Store.keep(text(d["note"] ?? ""))
    return "ok"
  }
}
