space quarry.alerts
use quarry.streaks

// The third space to layer the store, and the only one that reacts rather
// than decides. It takes the `announce` seam — where an incident that has
// just opened passes — and puts it on a named channel (§9.5). The board's
// ticker subscribes to that same name. Neither space imports the other;
// they meet at a text, which is the one place in Ashlar a name is data.
//
// It also joins the boot sequence. `boot` and `wind` are `stack` and
// `stack reverse`, so this layer starts AFTER quarry.data's and stops
// BEFORE it — lifecycle is composition order, not a separate concept
// (§4).
part Feed {
  channel = "quarry.alerts"
}

part quarry.data.Store {
  tags append = ["alerts"]

  announce pipe = (i: quarry.data.Incident) => {
    publish(quarry.alerts.Feed.channel, nameOf(i.line) + " tripped — " + i.why)
    log.warn("quarry: incident opened", { line: i.line, why: i.why })
    return i
  }

  boot stack = () => {
    log.info("quarry: alerts armed", { channel: quarry.alerts.Feed.channel })
    return none
  }

  wind stack reverse = () => {
    log.info("quarry: alerts disarmed")
    return none
  }
}
