space demo

part item {
  route = "/api/things/{id}"
  handle pipe = (req: std.Request) => req.path
}

part maker {
  route = "/api/things/new"
  handle pipe = (req: std.Request) => req.path
}
