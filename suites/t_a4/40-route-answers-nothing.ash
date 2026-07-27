// A route has to answer with something. This used to compile clean and
// serve a JSON dump of `std.Request` — headers included — a response form
// §9.2 does not define, reached by falling through every branch that does.
space probe

part app {
  port = 8080
}

part bare {
  route = "/bare"
}
