space srv

part Server {
  port = 8080
  stop stack reverse = () => {
    log.info("base stopping")
    return none
  }
}

// file: b.ash
space srv.metrics
use srv

part srv.Server {
  stop stack reverse = () => {
    log.info("metrics stopping")
    return none
  }
}
