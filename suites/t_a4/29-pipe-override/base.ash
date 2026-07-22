space chat.api

part messages {
  route = "/api/messages"
  handle pipe = (req: std.Request) => req
}
