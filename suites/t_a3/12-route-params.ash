space chat.api

part message {
  route = "/api/messages/{id}"
  handle pipe = (req: std.Request) => req.params["id"]
}
