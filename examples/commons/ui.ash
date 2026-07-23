space commons.ui
use commons.data

// The interface. Every view part renders semantic HTML and carries only
// `class` names for appearance — those names bind to commons.css by name
// (§9.4), the same way every other name in Ashlar is the joint. No
// element carries a style string; presentation is a named boundary, not
// a value smuggled through the type system.

// A logged-out visitor. Both forms are NATIVE posts — no handler, no
// socket — so the browser does the round-trip and the runtime sets the
// session cookie (§9.6). This is the whole login stack, zero client code.
part gate {
  view = () => el("div", { class: "gate" }, [
    el("div", { class: "card" }, [
      el("h1", { class: "wordmark" }, ["commons"]),
      el("p", { class: "muted" }, ["a place for the team to talk"]),
      el("form", { class: "stack", action: "/api/login", method: "post" }, [
        el("h2", {}, ["log in"]),
        el("input", { class: "field", name: "email", type: "email", placeholder: "you@team.dev" }, []),
        el("input", { class: "field", name: "password", type: "password", placeholder: "password" }, []),
        el("button", { class: "primary" }, ["log in"]),
      ]),
      el("form", { class: "stack", action: "/api/signup", method: "post" }, [
        el("h2", {}, ["or sign up"]),
        el("input", { class: "field", name: "name", type: "text", placeholder: "your name" }, []),
        el("input", { class: "field", name: "email", type: "email", placeholder: "you@team.dev" }, []),
        el("input", { class: "field", name: "password", type: "password", placeholder: "password" }, []),
        el("button", { class: "primary" }, ["create account"]),
      ]),
    ]),
  ])
}

// The authenticated frame. It carries the viewer's id and the selected
// room down as fields, mounts the presence probe and the notice tray,
// and shows the chosen room (or a prompt when none is selected).
part shell {
  uid: text
  rid: text
  view = () => el("div", { class: "app" }, [
    el(commons.ui.presence, { uid: uid }),
    el(commons.ui.sidebar, { uid: uid, rid: rid }),
    el("main", { class: "main" }, pane()),
    el(commons.ui.notices, { uid: uid }),
  ])
  pane = () => {
    return if rid != "" { [el(commons.ui.channel, { uid: uid, sender: commons.data.Store.nameOf(uid), rid: rid })] } else { [
      el("div", { class: "empty" }, [
        el("p", { class: "muted" }, ["pick a room on the left, or start a new one."]),
      ]),
    ] }
  }
}

// Presence by lifecycle (§9.5): mounting a page arrives, its socket
// closing departs. No heartbeat, no polling — the instance's own life
// IS the signal, and every sidebar reading `online` re-renders on it.
part presence {
  uid: text
  view = () => el("span", { class: "probe" }, [])
  start stack = () => {
    commons.data.Store.arrive(uid)
    return none
  }
  stop stack reverse = () => {
    commons.data.Store.depart(uid)
    return none
  }
}

part sidebar {
  uid: text
  rid: text
  view = () => el("aside", { class: "sidebar" }, [
    el("div", { class: "me" }, [
      el("span", { class: "dot on" }, []),
      commons.data.Store.nameOf(uid),
    ]),
    el("div", { class: "section" }, ["rooms"]),
    el("nav", { class: "rooms" }, roomLinks()),
    el("form", { class: "newroom", action: "/api/rooms", method: "post" }, [
      el("input", { class: "field", name: "name", placeholder: "new room" }, []),
      el("input", { class: "field", name: "purpose", placeholder: "purpose" }, []),
      el("button", { class: "ghost" }, ["add room"]),
    ]),
    el("div", { class: "section" }, ["online"]),
    el("nav", { class: "people" }, peopleLinks()),
    el("a", { class: "logout", href: "/api/logout" }, ["log out"]),
  ])
  roomLinks = () => map(commons.data.Store.roomsFor(uid), (r: commons.data.Room) => el(commons.ui.roomLink, {
    rid: r.id,
    label: r.name,
    kind: r.kind,
    uid: uid,
    current: rid,
  }))
  peopleLinks = () => map(commons.data.Store.onlineList(), (who: commons.data.Person) => el(commons.ui.personLink, {
    who: who.id,
    label: who.name,
    me: uid,
  }))
}

part roomLink {
  rid: text
  label: text
  kind: text
  uid: text
  current: text
  view = () => el("a", { class: cls(), href: "/c/" + rid }, [
    el("span", { class: "roomname" }, [mark() + label]),
    badge(),
  ])
  mark = () => (if kind == "dm" { "@ " } else { "# " })
  cls = () => (if rid == current { "roomlink active" } else { "roomlink" })
  badge = () => {
    let n = commons.data.Store.unreadIn(uid, rid)
    return if n > 0 and rid != current { el("span", { class: "badge" }, [text(n)]) } else { el("span", { class: "spacer" }, []) }
  }
}

part personLink {
  who: text
  label: text
  me: text
  view = () => {
    return if who != me { el("a", { class: "person", href: "/dm/" + who }, [
      el("span", { class: "dot on" }, []),
      label,
    ]) } else { el("div", { class: "person self" }, [
      el("span", { class: "dot on" }, []),
      label + " (you)",
    ]) }
  }
}

// One room: header, live feed, and a composer whose submit runs on the
// server over the socket (§9.4). Mounting marks the room read for this
// viewer (§9.3), so its unread badge clears the moment they open it.
part channel {
  uid: text
  sender: text
  rid: text
  state draft: text = ""
  start stack = () => {
    commons.data.Store.markRead(uid, rid)
    return none
  }
  view = () => el("div", { class: "channel" }, [
    el("header", { class: "chanhead" }, [
      el("h2", {}, [heading()]),
      el("p", { class: "muted" }, [purpose()]),
    ]),
    el(commons.ui.feed, { rid: rid, me: uid }),
    el("form", { class: "composer", onsubmit: send }, [
      el("input", { class: "field grow", oninput: typed, value: draft, placeholder: "message " + heading() }, []),
      el("button", { class: "primary" }, ["send"]),
    ]),
  ])
  heading = () => {
    let r = commons.data.Store.rooms[rid]
    return if r != none { r!.name } else { "unknown room" }
  }
  purpose = () => {
    let r = commons.data.Store.rooms[rid]
    return if r != none { r!.purpose } else { "" }
  }
  typed = (e: std.Event) => {
    draft = text(e.data.value)
  }
  send = () => {
    commons.data.Store.send(rid, uid, sender, draft)
    draft = ""
  }
}

// The feed reads the store, so any post — from this client, another
// client, or a layered space's reaction — re-renders it live.
part feed {
  rid: text
  me: text
  view = () => el("div", { class: "feed" }, rows())
  rows = () => map(commons.data.Store.postsIn(rid), (p: commons.data.Post) => el(commons.ui.postRow, {
    who: p.authorName,
    body: p.body,
    side: side(p.author),
  }))
  side = (author: text) => (if author == me { "mine" } else { "theirs" })
}

part postRow {
  who: text
  body: text
  side: text
  view = () => el("div", { class: "post " + side }, [
    el("span", { class: "who" }, [who]),
    el("span", { class: "bubble" }, [body]),
  ])
}

// The notice tray subscribes to this viewer's mention channel by name
// (§9.5). commons.mentions publishes to the same name — the two spaces
// meet at a channel string neither imports from the other.
part notices {
  uid: text
  state items: [text] = []
  start stack = () => {
    subscribe("commons.notify." + uid, heard)
    return none
  }
  heard = (m: data) => {
    items = [...items, text(m)]
  }
  view = () => {
    return if len(items) > 0 { el("div", { class: "notices" }, cards()) } else { el("div", { class: "notices" }, []) }
  }
  cards = () => map(recent(), (note: text) => el("div", { class: "toast" }, [note]))
  recent = () => {
    let n = len(items)
    let from = if n > 3 { n - 3 } else { 0 }
    return slice(items, from, n)
  }
}
