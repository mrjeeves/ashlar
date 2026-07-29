space mesh

// The mesh an Ashlar site lives on: who else is running this program, and
// how to show them. Vendor this space into a project (`ashlar vendor`) and a
// site gains a live roster in two lines — one in the root's `start`, one
// `el(mesh.grid, {})` wherever the people belong.
//
// Everything here crosses the one boundary (§9.10). The mesh node is installed
// once per machine and shared by everything on it, so the space name binds it:
// `mesh` derives to `ashlar mesh worker`, which drives the one control socket
// that node already exposes to its own clients. A `foreign.json` entry
// overrides that like any other space, and the mesh ships nothing on Ashlar's
// behalf.

// One other node on this mesh. `here` is connected right now, as against
// merely known — a roster remembers, presence does not. Three fields, and
// deliberately not a site count: what a peer SERVES is a separate question
// with a separate answer, below.
part Peer {
  id: text
  label: text
  here: bool
}

// This node's own place: the identity peers address it by, and the mesh it
// joined. `network` is the app's mesh id — the answer to "whose roster is
// this", and the reason two Ashlar apps on one machine need not share one.
//
// `reachable` is false on a machine with no mesh node running, and then `note`
// is the sentence that fixes it. Not having a mesh is an ordinary state of a
// machine, so it is answered rather than failed: a site serves either way, and
// the pages below say which it is instead of showing an empty roster that
// looks the same as a lonely one.
part Here {
  id: text
  label: text
  network: text
  peers: number
  reachable: bool
  note: text
}

// One mesh this node is on, and how many peers are on it. A node can be on
// several — its own fleet, an app's area — so "which mesh" is a list, not a
// fact, and `ashlar mesh` prints every row rather than guessing one.
part Network {
  id: text
  peers: number
}

// The roster is a collection the runtime tracks: a view that read it
// re-renders when it changes (§9.10). Nothing here polls. The node streams
// presence to its own clients and the worker is one of them, so a peer
// arriving pushes, and every page that read the roster patches — the mesh
// sets the pace, not a schedule guessing at it.
foreign here: () -> mesh.Here watches mesh.Peer
foreign peers: () -> [mesh.Peer] watches mesh.Peer
foreign enter: (network: text, label: text) -> mesh.Here updates mesh.Peer
foreign networks: () -> [mesh.Network] watches mesh.Peer

// -- Transmitting -------------------------------------------------------------
//
// The roster says who is here. This says something TO them. The mesh is the
// room: everyone holding its id is in it, so there is no host to admit anyone
// and nothing to be offline. That is why the mesh id is the whole of the
// secret — a program written with one is a program only its holders can join,
// and rolling a new one is how a group changes its locks.

// One line somebody said, or one thing the room noticed. `kind` is `chat`,
// `joined` or `left`; `mine` is whether this machine said it, because a room
// where your own words look like everyone else's is a log, not a conversation.
part Said {
  from: text
  who: text
  text: text
  at: number
  mine: bool
  kind: text
}

// `say` reaches the people who are here NOW: there is no server holding a
// message for somebody who is out, which is what serverless costs. What was
// said while you WERE here survives a restart — the worker writes the room
// down beside the project's other runtime state.
foreign say: (text: text) -> bool updates mesh.Said
foreign heard: () -> [mesh.Said] watches mesh.Said

// The conversation: messages, arrivals, and the files people put in the room,
// in the order they happened. Classes are the contract with the app's
// stylesheet (ADR-0010): `talk`, `line`, `mine`, `run-on`, `said`, `who`,
// `when`, `notice`, `drop`, `empty`.
part talk {
  view = () => el("div", { class: "talk" }, lines())
  lines = () => {
    let all = heard()
    if len(all) == 0 {
      return [el("p", { class: "empty" }, ["Nobody has said anything. Whoever holds this program can hear you."])]
    }
    // Indexed, because whether a line repeats its author's name depends on
    // the line before it — a wall of the same name six times is a log.
    return map(range(len(all)), (i: number) => one(all[i]!, all[i - 1]))
  }
  one = (s: mesh.Said, before: mesh.Said?) => (if s.kind == "chat" { said(s, follows(s, before)) } else if s.kind == "file" { dropped(s) } else { el("p", { class: "notice" }, [s.who + " " + s.kind]) })
  // A run-on is the same person still talking: same author, same kind, and
  // close enough in time that it reads as one turn.
  follows = (s: mesh.Said, before: mesh.Said?) => {
    if before == none {
      return false
    }
    let last = before!
    return last.kind == "chat" and last.who == s.who and s.at - last.at < 120000
  }
  said = (s: mesh.Said, run_on: bool) => el("div", {
    class: (if s.mine { "line mine" } else { "line" }) + (if run_on { " run-on" } else { "" }),
  }, [
    el("span", { class: "who" }, [if run_on { "" } else { s.who }]),
    el("div", { class: "said" }, [
      el("span", {}, [s.text]),
      el("span", { class: "when" }, [since(s.at)]),
    ]),
  ])
  // A file lands in the conversation where it was put, not in a drawer
  // beside it. Whether its bytes are here yet is the offer's business.
  dropped = (s: mesh.Said) => {
    let it = find(offered(), (o: mesh.Offer) => o.name == s.text and o.peer == s.from)
    if it == none {
      return el("p", { class: "notice" }, [s.who + " shared " + s.text])
    }
    return el("div", { class: if s.mine { "line mine" } else { "line" } }, [
      el("span", { class: "who" }, [s.who]),
      el("div", { class: "said drop" }, [
        el("span", { class: "drop-name" }, [it!.name]),
        el("span", { class: "drop-size" }, [size(it!.size)]),
        if it!.url == "" { el("button", { onclick: (e: std.Event) => fetch(it!.peer, it!.token, it!.name) }, ["get"]) } else { el("a", { href: it!.url }, ["open"]) },
      ]),
    ])
  }
  size = (bytes: number) => (if bytes < 1024 { text(round(bytes)) + " B" } else if bytes < 1048576 { text(round(bytes / 1024)) + " KB" } else { text(round(bytes / 1048576)) + " MB" })
  // How long ago, in the only unit that reads at a glance. `now()` is the
  // runtime's clock and `at` was stamped on the other side of the boundary,
  // so a clock that disagrees shows "now" rather than a negative age.
  since = (at: number) => ago((now() - at) / 1000)
  ago = (seconds: number) => (if seconds < 45 { "now" } else if seconds < 3600 { text(round(seconds / 60)) + "m" } else if seconds < 86400 { text(round(seconds / 3600)) + "h" } else { text(round(seconds / 86400)) + "d" })
  round = (n: number) => n - n % 1
}

// The line you type in.
part speak {
  state draft: text = ""
  view = () => el("form", { class: "speak", onsubmit: send }, [
    el("input", {
      class: "line-in",
      name: "line",
      value: draft,
      placeholder: "say something",
      autocomplete: "off",
      oninput: typed,
    }, []),
    el("button", { type: "submit" }, ["send"]),
  ])
  // An event carries the value of the element it fired on, and a form has
  // none — so the field mirrors as it is typed and the submit sends that.
  typed = (e: std.Event) => {
    draft = text(e.data.value ?? "")
  }
  send = () => {
    say(draft)
    draft = ""
  }
}

// The room, in one element. An app that wants a chat program writes
// `el(mesh.room, {})` and nothing else — a header of who is here, the
// conversation, and a line to type in. Every piece is still its own part for
// an app that would rather place them itself.
part room {
  view = () => el("div", { class: "room" }, [
    el(banner, {}),
    el(talk, {}),
    el(speak, {}),
  ])
}

// Who is here, and where "here" is. A room's header is the one place the
// mesh's own name belongs: it is the answer to "which room is this".
part banner {
  view = () => el("header", { class: "room-top" }, [
    el("div", { class: "room-who" }, faces_of()),
    el("p", { class: "room-where" }, [where()]),
  ])
  faces_of = () => yours() + map(peers(), (p: mesh.Peer) => face(p.label, p.here))
  // Your own face, when this machine has one. A blank circle for a node that
  // is not there is worse than no circle.
  yours = () => {
    let me = here()
    if me.label == "" {
      return []
    }
    return [face(me.label, true)]
  }
  // One letter is a face when there is no photograph, and the whole name is
  // the title — a key is not a name, but it is at least a stable one.
  face = (name: text, lit: bool) => el("span", {
    class: if lit { "who-face who-here" } else { "who-face" },
    title: name,
  }, [slice(name, 0, 1)])
  where = () => {
    let mine = here()
    if not mine.reachable {
      return mine.note
    }
    let others = len(filter(peers(), (p: mesh.Peer) => p.here))
    if others == 0 {
      return mine.network + " · nobody else here yet"
    }
    return mine.network + " · " + text(others) + (if others == 1 { " other here" } else { " others here" })
  }
}

// -- Passing things around ----------------------------------------------------
//
// A room's files are not a share. The uploader mints a token whose allow-list
// is the room's members, and the one request a peer may make is "fetch this
// token" — checked against that list every time. Nothing durable is granted to
// anyone, so nothing has to be revoked: membership IS the authorization, which
// is the same sentence as "the mesh id is the invite".

// One file somebody put up. `url` is empty until this machine has fetched it,
// and is then an ordinary path on this site — the bytes are here.
part Offer {
  peer: text
  who: text
  token: text
  name: text
  size: number
  url: text
}

// `offer` states this machine's WHOLE current list, so offering fewer paths is
// how a file is taken back down.
foreign offer: (paths: [text]) -> [mesh.Offer] updates mesh.Offer
foreign offered: () -> [mesh.Offer] watches mesh.Offer
foreign fetch: (peer: text, token: text, name: text) -> text updates mesh.Offer

// Fetched bytes land under the project's own assets, so serving them is the
// ordinary static-file part (§9.8) and nothing new. An app that carries the
// shelf below carries this too.
part held {
  route = "/room"
  files = "room"
}

// The shelf: what the room is offering, and a way to pull one here. A file
// nobody has fetched yet is a button; once its bytes are local it is a link.
// Classes: `mesh-shelf`, `mesh-offer`, `mesh-offer-who`, `mesh-empty`.
part shelf {
  view = () => el("div", { class: "mesh-shelf" }, items())
  items = () => {
    let all = offered()
    if len(all) == 0 {
      return [el("p", { class: "mesh-empty" }, ["Nothing on the shelf."])]
    }
    return map(all, (o: mesh.Offer) => el("div", { class: "mesh-offer" }, [
      el("span", { class: "mesh-offer-who" }, [o.who]),
      if o.url == "" { el("button", { onclick: (e: std.Event) => fetch(o.peer, o.token, o.name) }, [o.name]) } else { el("a", { href: o.url }, [o.name]) },
    ]))
  }
}

// -- Seeing each other --------------------------------------------------------
//
// Holding the room's id gets you into the room. It does NOT get you somebody's
// camera: the mesh node refuses a media route from anyone who is not owner,
// fleet, or shared with, and that refusal is right. So `allow` is a separate,
// deliberate act by the person at the machine — the one call here that leaves
// something durable behind — and `watch` is what the other side then does.
//
// The frames cross as JPEG, not H.264, so a page shows a peer with an ordinary
// `img` and no client code at all. `seq` moving is what makes the browser ask
// for the next one.

// One peer's camera as this machine last saw it. `note` carries the node's own
// sentence when there is nothing to see — usually that they have not shared.
part Seen {
  peer: text
  who: text
  url: text
  seq: number
  note: text
}

foreign allow: (peer: text) -> bool
foreign watch: (peer: text) -> [mesh.Seen] updates mesh.Seen
foreign seen: () -> [mesh.Seen] watches mesh.Seen

// Faces. Classes: `mesh-faces`, `mesh-face`, `mesh-face-who`, `mesh-empty`.
part faces {
  view = () => el("div", { class: "mesh-faces" }, tiles())
  tiles = () => {
    let all = seen()
    if len(all) == 0 {
      return [el("p", { class: "mesh-empty" }, ["Nobody is on camera."])]
    }
    return map(all, (s: mesh.Seen) => el("div", { class: "mesh-face" }, [
      if s.url == "" { el("p", { class: "mesh-empty" }, [if s.note == "" { "waiting for " + s.who } else { s.note }]) } else { el("img", { src: s.url + "?f=" + text(s.seq), alt: s.who }, []) },
      el("span", { class: "mesh-face-who" }, [s.who]),
    ]))
  }
}

// The two buttons that turn it on: let this room see me, and show me them.
part camera {
  view = () => el("div", { class: "mesh-camera" }, rows())
  rows = () => map(peers(), (p: mesh.Peer) => el("div", { class: "mesh-camera-row" }, [
    el("span", { class: "mesh-name" }, [p.label]),
    el("button", { onclick: (e: std.Event) => allow(p.id) }, ["let them see me"]),
    el("button", { onclick: (e: std.Event) => watch(p.id) }, ["show me them"]),
  ]))
}

// The app's own mesh. Both values are settings: the name and shape are
// source, the value is deployment's (§9.12) — so one program can be run on
// the shared area, on a customer's private one, or on a throwaway for a
// test, without editing a line.
//
// An app that wants its own roster rather than the shared one says so in
// source by layering this part, which is the same replace the language uses
// everywhere:
//
//   part mesh.Mesh {
//     setting network: text = "my-app"
//   }
//
// and deployment can still override the value it chose.
// `label` names the APP on the mesh — the network it joins, the site it
// publishes. It is never the machine's name: the node's own identity belongs
// to whoever runs it, and no program that joins a mesh renames the computer
// it is running on.
part Mesh {
  setting network: text = "ashlar"
  setting label: text = "an ashlar site"
  state node: text = ""
  state joined: bool = false

  // Called from the server root's `start` stack — one line, and the site is
  // on the mesh before it answers a request:
  //
  //   start stack = () => {
  //     mesh.Mesh.arrive()
  //     return none
  //   }
  arrive = () => {
    let h = enter(network, label)
    node = h.id
    joined = true
  }
}

// The base grid: every peer on this app's mesh, live. Drop it into any view.
// Appearance binds by name (ADR-0010) — the classes below are the contract
// with the app's stylesheet, and this space ships none of its own:
// `mesh-grid`, `mesh-peer`, `mesh-dot`, `mesh-dot-here`, `mesh-name`,
// `mesh-empty`.
part grid {
  view = () => el("div", { class: "mesh-grid" }, cards())
  cards = () => {
    let all = peers()
    if len(all) > 0 {
      return map(all, (p: mesh.Peer) => el("div", { class: "mesh-peer" }, [
        el("span", { class: if p.here { "mesh-dot mesh-dot-here" } else { "mesh-dot" } }, []),
        el("span", { class: "mesh-name" }, [p.label]),
      ]))
    }
    return [el("p", { class: "mesh-empty" }, [empty()])]
  }
  // An empty roster has two causes and they are not the same news. Only ask
  // which when the roster IS empty: the answer costs a call, and the common
  // case never needs it.
  empty = () => {
    let h = here()
    if h.reachable {
      return "No one else here yet."
    }
    return "No mesh node is running on this machine, so there is no roster."
  }
}

// The settings panel: what this node is, which mesh it is on, and how many
// peers share it. A setting is immutable at runtime (§9.12), so this shows
// the values in force rather than editing them — the file or the layer is
// where they change. Route it, embed it, or leave it out; that is the
// developer's call.
part panel {
  view = () => el("div", { class: "mesh-panel" }, rows())
  rows = () => {
    let h = here()
    if not h.reachable {
      return [
        row("mesh", h.network),
        row("this node", "no mesh node is running here"),
        el("p", { class: "mesh-empty" }, [h.note]),
      ]
    }
    return [
      row("mesh", h.network),
      row("this node", h.label),
      row("id", h.id),
      row("peers", text(h.peers)),
    ]
  }
  row = (name: text, value: text) => el("div", { class: "mesh-row" }, [
    el("span", { class: "mesh-key" }, [name]),
    el("span", { class: "mesh-value" }, [value]),
  ])
}

// -- Sites --------------------------------------------------------------------
//
// The other half of what the mesh answers: this site, published to the peers,
// and theirs, reachable from here. `ashlar run --mesh` publishes the port it is
// already serving through the same capability; a program calls the names below
// when it wants to SHOW what is out there.

// One openable site. `url` is where this machine reaches it — a local
// address the proxy already bound, never something source wrote down (B5).
part Site {
  peer: text
  label: text
  url: text
}

// Where a site landed: the node peers address it by, and the mesh they must
// be on to reach it.
part Published {
  node: text
  network: text
  label: text
}

// Both lists move with the roster: a site appears when the peer serving it
// does, so `mesh.Peer` is the collection that marks them changed (§9.10).
foreign published: () -> [mesh.Site] watches mesh.Peer
foreign nearby: () -> [mesh.Site] watches mesh.Peer

// `ashlar run --mesh` calls these two on the program's behalf, which is why
// they are declared here rather than living only inside the runtime: the
// contract is source, readable and shape-checked, and `ashlar foreign check`
// covers it. A program may call them itself to publish a port the runtime did
// not bind. `network` empty means the shared area.
foreign expose: (port: number, label: text, network: text) -> mesh.Published updates mesh.Peer
foreign unexpose: (port: number) -> bool updates mesh.Peer

// The browser: every site the peers are running, as links that open. Classes
// are the contract with the app's stylesheet (ADR-0010): `mesh-sites-list`,
// `mesh-site`, `mesh-site-peer`, `mesh-empty`.
part browser {
  view = () => el("div", { class: "mesh-sites-list" }, links())
  links = () => {
    let all = nearby()
    if len(all) > 0 {
      return map(all, (s: mesh.Site) => el("a", { class: "mesh-site", href: s.url }, [
        el("span", {}, [s.label]),
        el("span", { class: "mesh-site-peer" }, [s.peer]),
      ]))
    }
    return [el("p", { class: "mesh-empty" }, [empty()])]
  }
  empty = () => {
    let h = here()
    if h.reachable {
      return "No sites on this mesh yet."
    }
    return "No mesh node is running on this machine, so there is nothing to browse."
  }
}
