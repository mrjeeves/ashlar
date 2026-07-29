space enclave
use mesh

// enclave — a site that is only reachable to the people on its own mesh.
//
// Nothing here knows an address. The program joins a mesh by NAME, the mesh
// says who else is on it, and `ashlar run --mesh` publishes this origin to
// them. Where any of that actually is stays a deployment fact (B5).
part app {
  port = 8080
  style = "enclave"
  start stack = () => {
    mesh.Mesh.arrive()
    return none
  }
}

// This app's own mesh, decided in source. Layering the vendored part is the
// ordinary replace (§4): the roster below belongs to `enclave` and does not
// inherit the shared area every unconfigured Ashlar site lands on. Deployment
// can still override the value — a customer's private mesh, or a throwaway
// for a test — because it is a setting (§9.12).
part mesh.Mesh {
  setting network: text = "enclave"
  setting label: text = "enclave"
}

part page {
  route = "/"
  view = () => el("div", { class: "stage" }, [
    el("title", {}, ["enclave"]),
    el("div", { class: "card" }, [
      el("p", { class: "kicker" }, ["private mesh · §9.10"]),
      el("h1", {}, ["enclave"]),
      el("p", { class: "lede" }, ["A room for the people who hold this program's mesh id, and nobody else. No server in the middle, no account to make, no address anyone wrote down. Everything below is server-rendered and arrives when it happens."]),
      el(mesh.talk, {}),
      el(mesh.speak, {}),
      el("h2", {}, ["Who is here"]),
      el(mesh.grid, {}),
      el("h2", {}, ["This node"]),
      el(mesh.panel, {}),
      // The same mesh can carry the origin itself, which is the other half
      // of §9.10 and a smaller point than the room: a site published to
      // these people is reachable to them and to nobody else.
      el("h2", {}, ["Sites on this mesh"]),
      el(mesh.browser, {}),
    ]),
  ])
}

// The same roster over HTTP, so a client is not required to be a browser
// (§9.2). One handler, two transports.
part api {
  route = "/api/peers"
  handle pipe = (req: std.Request) => mesh.peers()
}
