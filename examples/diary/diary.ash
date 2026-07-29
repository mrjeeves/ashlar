space diary

// Auth end to end: signup/login/logout builtins, an `allow` guard, and
// a per-user page — sessions ride an HttpOnly cookie (§9.6).
part app {
  port = 8080
  style = "diary"
}

// The front page renders a view, never a redirect, so both branches share
// one shape: a signed-in visitor reads their diary, everyone else meets
// the login gate. Identity crosses from the request into the view (§9.4).
part home {
  route = "/"
  handle pipe = (req: std.Request) => {
    return if req.user != none { el(reader, { who: req.user!.email }) } else { el(gate, {}) }
  }
}

// One line somebody left. A data shape: fields only (§5).
part Note {
  who: text
  line: text
}

// What the signed-in share. `present` is reference-counted off the view
// lifecycle (§9.4): a page mounting arrives, its socket closing departs, so
// the list is live with no heartbeat. `book` is what they wrote, and both are
// ordinary shared state — the identity that wrote a line came from the
// request (§9.6), and once written it belongs to everybody.
part Lobby {
  state present: {text: number} = {}
  state book: [diary.Note] = []
  arrive = (who: text) => {
    present = put(present, who, (present[who] ?? 0) + 1)
  }
  depart = (who: text) => {
    let n = (present[who] ?? 0) - 1
    if n <= 0 {
      present = drop(present, who)
    } else {
      present = put(present, who, n)
    }
  }
  write = (who: text, line: text) => {
    let tail = slice(book, 0, 7)
    book = [{ who: who, line: line }, ...tail]
  }
}

part reader {
  who: text
  state draft: text = ""

  start stack = () => {
    Lobby.arrive(who)
    return none
  }
  stop stack reverse = () => {
    Lobby.depart(who)
    return none
  }

  view = () => el("div", { class: "stage" }, [
    el("title", {}, ["diary"]),
    el("div", { class: "card" }, [
      el("p", { class: "kicker" }, ["sessions · §9.6"]),
      el("h1", {}, ["diary"]),
      el("p", { class: "entry" }, ["dear diary, from " + who]),
      el("form", { class: "row", onsubmit: leave_line }, [
        el("input", { class: "field", oninput: typed, value: draft, placeholder: "leave a line for whoever is next", autocomplete: "off" }, []),
        el("button", { class: "primary" }, ["sign it"]),
      ]),
      el("p", { class: "lobbytitle" }, ["Signed in right now"]),
      el("div", { class: "faces" }, faces()),
      el("ul", { class: "book" }, lines()),
      el("a", { class: "ghost", href: "/leave" }, ["log out"]),
    ]),
  ])

  // Every signed-in page read `present`, so every one of them re-renders
  // when somebody else signs in or closes their tab.
  faces = () => map(keys(Lobby.present), (e: text) => el("span", { class: if e == who { "face you" } else { "face" } }, [e]))
  lines = () => {
    if len(Lobby.book) == 0 {
      return [el("li", { class: "bookempty" }, ["Nothing written yet."])]
    }
    return map(Lobby.book, (n: diary.Note) => el("li", { class: "bookrow" }, [
      el("span", { class: "bookwho" }, [n.who]),
      el("span", { class: "bookline" }, [n.line]),
    ]))
  }

  typed = (e: std.Event) => {
    draft = text(e.data.value ?? "")
  }
  leave_line = () => {
    if draft != "" {
      Lobby.write(who, draft)
    }
    draft = ""
  }
}

// Both forms are native posts — no handler, no socket — so the browser
// does the round-trip and the runtime sets the session cookie (§9.6).
part gate {
  view = () => el("div", { class: "stage" }, [
    el("div", { class: "card" }, [
      el("p", { class: "kicker" }, ["sessions · §9.6"]),
      el("h1", {}, ["diary"]),
      el("p", { class: "lede" }, ["A private page behind a login. Sign up, and the entry is yours alone."]),
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

// Browser-facing auth: run the builtin, then redirect home so a fresh page
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

// The API surface returns the builtins' own results (§9.2), the raw auth
// primitives a programmatic client wants.
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
