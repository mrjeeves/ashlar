space commons.data

// The domain shapes. A data shape is a part of typed fields only; its
// values are plain literals, checked against the fields wherever one is
// built (§5). Identity, a room (a channel or a DM), one posted message.
part Person {
  id: text
  name: text
  email: text
}

part Room {
  id: text
  name: text
  purpose: text
  kind: text
  members: [text]
}

part Post {
  id: text
  roomId: text
  author: text
  authorName: text
  body: text
  sent: number
}

// The one shared store every space reads and the whole team sees.
// `stored` state survives restarts; `synced` state is pushed live to
// every view that read it. The two pipe seams — `prepare` (body) and
// `announce` (a posted message) — are where independently owned spaces
// layer policy and reactions WITHOUT editing this file: commons.moderation
// stacks onto `prepare`, commons.mentions onto `announce` (§4).
part Store {
  stored people: {text: commons.data.Person} = {}
  stored rooms: {text: commons.data.Room} = {}
  stored posts: {text: commons.data.Post} = {}
  stored reads: {text: {text: number}} = {}
  synced online: {text: number} = {}

  prepare pipe = (body: text) => slice(body, 0, 400)
  announce pipe = (p: commons.data.Post) => p

  setProfile = (uid: text, name: text, email: text) => {
    people = put(people, uid, { id: uid, name: name, email: email })
  }

  nameOf = (uid: text) => {
    let who = people[uid]
    return if who != none { who!.name } else { "someone" }
  }

  createRoom = (name: text, purpose: text) => {
    let rid = id()
    rooms = put(rooms, rid, {
      id: rid,
      name: name,
      purpose: purpose,
      kind: "room",
      members: [],
    })
    return rid
  }

  // A room with a fixed id, created once. The seed room uses this so a
  // fresh install always has somewhere to talk.
  ensureRoom = (rid: text, name: text, purpose: text) => {
    if rooms[rid] == none {
      rooms = put(rooms, rid, {
        id: rid,
        name: name,
        purpose: purpose,
        kind: "room",
        members: [],
      })
    }
  }

  // A DM is just a room whose id is derived from the two members, so
  // asking twice returns the same room instead of a second one.
  openDm = (a: text, b: text) => {
    let pair = sort([a, b], (u: text) => u)
    let rid = "dm-" + join(pair, "-")
    if rooms[rid] == none {
      rooms = put(rooms, rid, {
        id: rid,
        name: nameOf(a) + " and " + nameOf(b),
        purpose: "direct messages",
        kind: "dm",
        members: pair,
      })
    }
    return rid
  }

  send = (rid: text, author: text, authorName: text, body: text) => {
    let clean = prepare(body)
    if clean != "" {
      let pid = id()
      posts = put(posts, pid, {
        id: pid,
        roomId: rid,
        author: author,
        authorName: authorName,
        body: clean,
        sent: now(),
      })
      announce(posts[pid]!)
    }
  }

  // Presence is reference-counted: a page arriving increments the
  // user's live connection count, its socket closing decrements it, and
  // every sidebar reading `online` re-renders on the change (§9.5).
  arrive = (uid: text) => {
    online = put(online, uid, (online[uid] ?? 0) + 1)
  }
  depart = (uid: text) => {
    let n = (online[uid] ?? 0) - 1
    if n <= 0 {
      online = drop(online, uid)
    } else {
      online = put(online, uid, n)
    }
  }

  markRead = (uid: text, rid: text) => {
    let mine = reads[uid] ?? {}
    reads = put(reads, uid, put(mine, rid, now()))
  }

  // Derived reads. Views call these, so the reads they perform on
  // `posts`/`reads`/`rooms` are what makes the views reactive.
  postsIn = (rid: text) => {
    let here = filter(map(keys(posts), (k: text) => posts[k]!), (p: commons.data.Post) => p.roomId == rid)
    return sort(here, (p: commons.data.Post) => p.sent)
  }

  unreadIn = (uid: text, rid: text) => {
    let mine = reads[uid] ?? {}
    let last = mine[rid] ?? 0
    return len(filter(map(keys(posts), (k: text) => posts[k]!), (p: commons.data.Post) => p.roomId == rid and p.sent > last))
  }

  roomsFor = (uid: text) => {
    let all = map(keys(rooms), (k: text) => rooms[k]!)
    return filter(all, (r: commons.data.Room) => r.kind == "room" or contains(r.members, uid))
  }

  onlineList = () => {
    return filter(map(keys(people), (k: text) => people[k]!), (who: commons.data.Person) => online[who.id] ?? 0 > 0)
  }
}
