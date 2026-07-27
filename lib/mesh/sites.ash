space mesh.sites
use mesh

// Sites on the mesh: this one, published to the peers, and theirs, reachable
// from here. A site is a whole running program, so this is the half that
// needs a proxy on the machine — which is why it is its own space with its
// own binding. A project that only wants the roster (`mesh`) never installs
// it, and `ashlar foreign check` says so rather than a request finding out.
//
// `ashlar run --mesh` publishes the site it is already serving through this
// same capability; a program calls the names below when it wants to SHOW
// what is out there.

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
foreign published: () -> [mesh.sites.Site] watches mesh.Peer
foreign nearby: () -> [mesh.sites.Site] watches mesh.Peer

// `ashlar run --mesh` calls these two on the program's behalf, which is why
// they are declared here rather than living only inside the runtime: the
// contract a mesh must implement is source, readable and shape-checked, and
// `ashlar foreign check` covers it. A program may call them itself to publish
// a port the runtime did not bind. `network` empty means the mesh the
// machine's daemon calls its own.
foreign expose: (port: number, label: text, network: text) -> mesh.sites.Published updates mesh.Peer
foreign unexpose: (port: number) -> bool updates mesh.Peer

// The browser: every site the peers are running, as links that open. Classes
// are the contract with the app's stylesheet (ADR-0010): `mesh-sites-list`,
// `mesh-site`, `mesh-site-peer`, `mesh-empty`.
part browser {
  view = () => el("div", { class: "mesh-sites-list" }, links())
  links = () => {
    let all = nearby()
    if len(all) == 0 {
      return [el("p", { class: "mesh-empty" }, ["No sites on this mesh yet."])]
    }
    return map(all, (s: mesh.sites.Site) => el("a", { class: "mesh-site", href: s.url }, [
      el("span", {}, [s.label]),
      el("span", { class: "mesh-site-peer" }, [s.peer]),
    ]))
  }
}
