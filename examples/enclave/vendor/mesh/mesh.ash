space mesh

// The mesh an Ashlar site lives on: who else is running this program, and
// how to show them. Vendor this space into a project (`ashlar vendor`) and a
// site gains a live roster in two lines — one in the root's `start`, one
// `el(mesh.grid, {})` wherever the people belong.
//
// Everything here crosses the one boundary (§9.10). The mesh daemon is
// installed once per machine and shared by everything on it, so the space
// name binds it: `mesh` derives to a co-process, and a `foreign.json` entry
// overrides that like any other space.

// One other node on this mesh. `here` is connected right now, as against
// merely known — a roster remembers, presence does not.
part Peer {
  id: text
  label: text
  here: bool
  sites: number
}

// This node's own place: the identity peers address it by, and the mesh it
// joined. `network` is the app's mesh id — the answer to "whose roster is
// this", and the reason two Ashlar apps on one machine need not share one.
part Here {
  id: text
  label: text
  network: text
  peers: number
}

// The roster is a collection the runtime tracks: a view that read it
// re-renders when `reread` marks it changed (§9.10). `revision` is outside
// that on purpose — the poll below asks it every few seconds, and a call
// that marked the collection every time it was asked would re-render every
// connected page on a schedule instead of on a change.
foreign here: () -> mesh.Here watches mesh.Peer
foreign peers: () -> [mesh.Peer] watches mesh.Peer
foreign enter: (network: text, label: text) -> mesh.Here updates mesh.Peer
foreign revision: () -> number
foreign reread: () -> number updates mesh.Peer

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

// Presence is polled because a co-process answers questions; it does not
// push (§9.10). Asking for the revision is cheap and marks nothing, so the
// roster re-renders when it changed and not when it was merely asked about.
// An app that wants a different cadence layers this part's `every`.
part Watch {
  every = "3s"
  state seen: number = 0
  run = () => {
    let rev = revision()
    if rev != seen {
      seen = reread()
    }
  }
}

// The base grid: every peer on this app's mesh, live. Drop it into any view.
// Appearance binds by name (ADR-0010) — the classes below are the contract
// with the app's stylesheet, and this space ships none of its own:
// `mesh-grid`, `mesh-peer`, `mesh-dot`, `mesh-dot-here`, `mesh-name`,
// `mesh-sites`, `mesh-empty`.
part grid {
  view = () => el("div", { class: "mesh-grid" }, cards())
  cards = () => {
    let all = peers()
    if len(all) == 0 {
      return [el("p", { class: "mesh-empty" }, ["No one else here yet."])]
    }
    return map(all, (p: mesh.Peer) => el("div", { class: "mesh-peer" }, [
      el("span", { class: if p.here { "mesh-dot mesh-dot-here" } else { "mesh-dot" } }, []),
      el("span", { class: "mesh-name" }, [p.label]),
      el("span", { class: "mesh-sites" }, [text(p.sites) + " site(s)"]),
    ]))
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
