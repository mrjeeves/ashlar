// `every = 10` names no scale. It used to pass `check` unremarked and
// yield a task that never ran: statically decidable, undetected, and not
// in the reference's documented pair of undetectable faults.
space probe

part app {
  port = 8080
}

part sweep {
  every = 10
  run = () => {
    log.info("tick", {})
  }
}
