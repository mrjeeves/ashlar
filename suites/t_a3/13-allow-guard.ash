space chat.api

part profile {
  route = "/api/profile"
  allow = (req: std.Request) => req.user != none
  handle pipe = (req: std.Request) => req.user
}
