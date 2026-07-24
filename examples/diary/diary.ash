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

part reader {
  who: text
  view = () => el("div", { class: "stage" }, [
    el("div", { class: "card" }, [
      el("p", { class: "kicker" }, ["sessions · §9.6"]),
      el("h1", {}, ["diary"]),
      el("p", { class: "entry" }, ["dear diary, from " + who]),
      el("a", { class: "ghost", href: "/leave" }, ["log out"]),
    ]),
  ])
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

// The API surface returns the builtins' own results (§9.2), the raw auth
// primitives a programmatic client wants.
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
