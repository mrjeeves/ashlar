space hello

// A part with `port` is the server (§9.1). No main, no router, no
// wiring — names do all of it, and the build computes the rest.
part app {
  port = 8080
}

// A part with `route` is an endpoint; returning text answers the
// request. `handle` is a pipe so other spaces can layer on it (§9.2).
part greet {
  route = "/"
  handle pipe = (req: std.Request) => "hello from ashlar"
}
