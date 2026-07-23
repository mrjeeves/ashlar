space commons.mentions
use commons.moderation

// A second independently owned space. It layers `announce` — the seam a
// posted message runs through after it is stored — to find @name
// mentions and ping those people over a per-user channel. The notice
// tray in commons.ui subscribes to the same channel name; the two never
// import each other, they meet at the name (§9.5).
//
// It uses commons.moderation deliberately: that `use` orders these two
// layers of Store (mentions after moderation), so a mention is scanned
// against the already-redacted body — the language's answer to two
// independent layers is to name their order, not guess it (§3).
part commons.data.Store {
  announce pipe = (p: commons.data.Post) => {
    for who in map(keys(people), (k: text) => people[k]!) {
      if contains(p.body, "@" + who.name) {
        publish("commons.notify." + who.id, p.authorName + " mentioned you in " + roomName(p.roomId))
      }
    }
    return p
  }

  roomName = (rid: text) => {
    let r = rooms[rid]
    return if r != none { r!.name } else { "a room" }
  }
}
