space api

part item {
  route = "/x/{id}/{id}"
  handle pipe = (req: std.Request) => req.path
}
