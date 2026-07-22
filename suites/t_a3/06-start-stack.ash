space srv

part Server {
  port = 8080
  state ready: bool = false
  state count: number = 0
  start stack = () => {
    return { ready: true }
  }
}

// file: b.ash
space srv.metrics
use srv

part srv.Server {
  start stack = () => {
    return { count: 1 }
  }
}
