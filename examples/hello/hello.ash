space hello

part app {
  port = 8080
}

part greet {
  route = "/"
  handle pipe = (req: std.Request) => "hello from ashlar"
}
