space commons.app
use commons.ui

// The server root. `style = "commons"` names the stylesheet the runtime
// links into every served page (§9.4): the build resolves it to
// assets/commons.css, and a missing sheet is a loud error — a name in
// source, a location found by the build, exactly like `files` and
// `foreign`. The start stack seeds a room so a fresh install has one.
part app {
  port = 8080
  style = "commons"
  start stack = () => {
    commons.data.Store.ensureRoom("general", "general", "the whole team")
    log.info("commons is up")
    return none
  }
}

// The two page routes render views, never redirects, so their branches
// share one shape: a signed-in viewer gets the shell, everyone else gets
// the login gate. Identity crosses from the request into the view as
// fields on `el` (§9.4) — the bridge from `req.user` to a rendered page.
part home {
  route = "/"
  handle pipe = (req: std.Request) => {
    return if req.user != none { el(commons.ui.shell, { uid: req.user!.id, rid: "" }) } else { el(commons.ui.gate, {}) }
  }
}

part roomPage {
  route = "/c/{rid}"
  handle pipe = (req: std.Request) => {
    return if req.user != none { el(commons.ui.shell, { uid: req.user!.id, rid: req.params["rid"]! }) } else { el(commons.ui.gate, {}) }
  }
}

// The action routes are native form/link targets; each returns a single
// redirect, so a fresh page renders with the new state already in place.
part doSignup {
  route = "/api/signup"
  handle pipe = (req: std.Request) => {
    let who = signup(text(req.data.email), text(req.data.password))
    commons.data.Store.setProfile(who.id, text(req.data.name), text(req.data.email))
    return redirect("/")
  }
}

part doLogin {
  route = "/api/login"
  handle pipe = (req: std.Request) => {
    login(text(req.data.email), text(req.data.password))
    return redirect("/")
  }
}

part doLogout {
  route = "/api/logout"
  handle pipe = (req: std.Request) => {
    logout()
    return redirect("/")
  }
}

part startDm {
  route = "/dm/{other}"
  handle pipe = (req: std.Request) => {
    if req.user == none {
      return redirect("/")
    }
    return redirect("/c/" + commons.data.Store.openDm(req.user!.id, req.params["other"]!))
  }
}

part makeRoom {
  route = "/api/rooms"
  handle pipe = (req: std.Request) => {
    if req.user == none {
      return redirect("/")
    }
    return redirect("/c/" + commons.data.Store.createRoom(text(req.data.name), text(req.data.purpose)))
  }
}
