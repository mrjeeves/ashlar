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
      el("p", { class: "lede" }, ["Everyone holding this program's mesh id is in this room, and nobody else can find it. No server in the middle, no account, no address anyone wrote down."]),
      el(mesh.faces, {}),
      el(mesh.talk, {}),
      el(mesh.speak, {}),
      el("div", { class: "aside" }, [
        el(mesh.grid, {}),
        el(mesh.shelf, {}),
        el(mesh.camera, {}),
        el(mesh.panel, {}),
      ]),
    ]),
  ])
}

// The same roster over HTTP, so a client is not required to be a browser
// (§9.2). One handler, two transports.
part api {
  route = "/api/peers"
  handle pipe = (req: std.Request) => mesh.peers()
}
