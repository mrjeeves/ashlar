space slate.data

// slate — a shared pad. Open a URL, type, and whoever else has it open
// sees your text as you write it. No account, no session, no invite: the
// URL is the permission, which is what makes a pad a pad.
//
// The whole example exists for one problem: TWO PEOPLE TYPING AT ONCE.
// Everything else here — presence, revisions, size limits — is ordinary.
// That one is not, and it is the reason a pad is real software rather
// than a form with a save button.

part Pad {
  key: text
  title: text
  body: text
  edits: number
  touched: number
}

part Revision {
  ref: text
  pad: text
  body: text
  at: number
  by: text
}

// One person's edit on its way in. It carries `base` — the text that
// person had in front of them when they typed — which is the fact that
// makes merging possible at all. Without it the server sees only "the
// field now reads X" and has no way to tell an insertion from a stale
// overwrite.
//
// `refused` is how a layer says no: the seam is a `pipe`, so every layer
// returns an Edit, and a policy that wants to reject fills this in rather
// than inventing a second return shape.
part Edit {
  pad: text
  base: text
  incoming: text
  who: text
  refused: text
}

part Merged {
  body: text
  clashes: number
}

part Store {
  stored pads: {text: slate.data.Pad} = {}
  stored revisions: {text: [slate.data.Revision]} = {}

  // Who is on which pad, by page. Presence is memory: it is true while a
  // socket is open and meaningless afterwards.
  state here: {text: {text: text}} = {}

  // Which line each person's caret is on, per pad. Lines, not offsets: the
  // merge works line by line (ADR-0028), so the line is the granularity at
  // which "where are you working" is a true answer rather than a decorative
  // one. Two carets on one line is exactly the collision the merge will have
  // to resolve, which makes it worth showing BEFORE it happens.
  state carets: {text: {text: number}} = {}
  state seq: number = 0
  state clashes: {text: number} = {}

  // The last few texts this pad held. A keystroke takes a moment to
  // arrive, and in that moment someone else's line can land — so an edit
  // can show up carrying a line the writer typed against text that has
  // already moved on. This is how the merge tells that apart from an
  // opinion (below).
  state recent: {text: [text]} = {}

  tags append: [text] = ["core"]
  limits deep: {text: {text: number}} = { pad: { revisions: 20 }, merge: { lines: 4096 } }

  // The seam every edit passes through before it lands (§4). Layers add
  // policy without editing this file: slate.limits caps the size,
  // slate.history snapshots a revision.
  apply pipe = (e: slate.data.Edit) => e

  // Naming the shape a literal is checked against (ADR-0025).
  hold = (m: slate.data.Merged) => m
  holdEdit = (e: slate.data.Edit) => e

  boot stack = () => {
    log.info("slate: pads online", { pads: len(keys(pads)) })
    return none
  }

  wind stack reverse = () => {
    log.info("slate: pads down")
    return none
  }

  // -- pads -------------------------------------------------------------

  ensurePad = (key: text, title: text) => {
    if pads[key] == none {
      pads = put(pads, key, {
        key: key,
        title: if title != "" { title } else { key },
        body: "",
        edits: 0,
        touched: now(),
      })
    }
  }

  bodyOf = (key: text) => {
    let p = pads[key]
    return if p != none { p!.body } else { "" }
  }

  titleOf = (key: text) => {
    let p = pads[key]
    return if p != none { p!.title } else { key }
  }

  padList = () => sort(map(keys(pads), (k: text) => pads[k]!), (p: slate.data.Pad) => 0 - p.touched)

  // -- the edit path ------------------------------------------------------

  // One keystroke's worth of work. The client says "I had BASE, now I have
  // INCOMING"; the server holds CURRENT, which may already contain someone
  // else's typing. Three texts, one answer.
  commit = (key: text, base: text, incoming: text, who: text) => {
    let p = pads[key]
    if p == none {
      return "no pad called " + key
    }
    let e = apply(holdEdit({
      pad: key,
      base: base,
      incoming: incoming,
      who: who,
      refused: "",
    }))
    if e.refused != "" {
      return e.refused
    }
    let merged = merge3(key, e.base, e.incoming, p!.body)
    pads = put(pads, key, {
      ...p!,
      body: merged.body,
      edits: p!.edits + 1,
      touched: now(),
    })
    if merged.clashes > 0 {
      clashes = put(clashes, key, (clashes[key] ?? 0) + merged.clashes)
      publish("slate.clash." + key, who + " and someone else typed on the same line; the copy already on the pad won")
    }
    remember(key, merged.body)
    // Every page holding this pad now has a new idea of what it says, so
    // tell them. Without this each page keeps measuring its next
    // keystroke against the text it had when IT last typed — a base that
    // goes stale the moment somebody else's line lands, which turns their
    // next perfectly ordinary edit into a phantom conflict. The patch
    // that updates their screen and the message that updates their base
    // are the same event, and they must not come apart.
    publish("slate.changed." + key, merged.body)
    return ""
  }

  // A three-way merge, line by line, and the reason two people can type at
  // once without either of them losing work.
  //
  // Last-write-wins is what falls out if you do nothing: the client sends
  // the whole field, so the slower typist's snapshot overwrites the faster
  // one's and their work vanishes with no error anywhere. That is the
  // naive design, and it is wrong in the way that matters — silently.
  //
  // So each line is decided against the base the writer actually had:
  // if they did not touch it, the pad's copy stands; if nobody else
  // touched it, theirs lands; if both changed it the same way, either
  // will do; and if both changed it differently, that is a real conflict.
  // The pad's copy wins and the writer is TOLD, because the one thing a
  // shared editor must never do is quietly discard what somebody typed.
  merge3 = (key: text, base: text, mine: text, current: text) => {
    if mine == base {
      return hold({ body: current, clashes: 0 })
    }
    let b = split(base, "\n")
    let m = split(mine, "\n")
    let c = split(current, "\n")
    let n = widest(len(b), len(m), len(c))
    return hold({
      body: join(fold(key, b, m, c, 0, n, []), "\n"),
      clashes: clashesIn(key, b, m, c, 0, n, 0),
    })
  }

  // Locals are single-assignment, so the accumulator is a parameter and
  // each call rebinds it — the same shape every walk in this language
  // takes (§7).
  fold = (key: text, b: [text], m: [text], c: [text], at: number, upto: number, out: [text]) => {
    if at >= upto {
      return out
    }
    return fold(key, b, m, c, at + 1, upto, out + [decide(key, at, row(b, at), row(m, at), row(c, at))])
  }

  clashesIn = (key: text, b: [text], m: [text], c: [text], at: number, upto: number, so_far: number) => {
    if at >= upto {
      return so_far
    }
    let bumped = if isClash(key, at, row(b, at), row(m, at), row(c, at)) { 1 } else { 0 }
    return clashesIn(key, b, m, c, at + 1, upto, so_far + bumped)
  }

  decide = (key: text, at: number, was: text, mine: text, theirs: text) => {
    if mine == was {
      return theirs
    }
    if mine == theirs {
      return mine
    }
    if theirs == was {
      // Nobody else touched this line, so the writer's version lands —
      // unless what they sent is the line the pad held one version ago.
      // Then their keystroke crossed somebody's patch in flight and they
      // are a step behind, and letting it land would silently undo the
      // other person's work.
      return if lagging(key, at, mine) { theirs } else { mine }
    }
    return theirs
  }

  // A real conflict is two people forming different opinions about one
  // line. A keystroke that crossed someone else's patch in flight is not
  // that: the writer is sending a line this pad held moments ago, because
  // their screen had not caught up when their finger came down. Both end
  // with the pad's copy standing, but only the first is worth telling
  // anyone about — and treating the second as a conflict would cry wolf
  // at every fast typist.
  isClash = (key: text, at: number, was: text, mine: text, theirs: text) => mine != was and theirs != was and mine != theirs and not lagging(key, at, mine)

  // One version back, and no further. A pad that refused anything it had
  // ever held could not be edited back to an earlier wording on purpose,
  // which is a thing people legitimately do; one step is the width of the
  // race this is here to close.
  lagging = (key: text, at: number, line: text) => {
    let seen = recent[key] ?? []
    if len(seen) < 2 {
      return false
    }
    return row(split(seen[len(seen) - 2]!, "\n"), at) == line
  }

  remember = (key: text, body: text) => {
    let seen = [...(recent[key] ?? []), body]
    recent = put(recent, key, if len(seen) > 6 { slice(seen, len(seen) - 6, len(seen)) } else { seen })
  }

  row = (xs: [text], at: number) => xs[at] ?? ""

  widest = (a: number, b: number, c: number) => {
    if a >= b and a >= c {
      return a
    }
    return if b >= c { b } else { c }
  }

  clashesOn = (key: text) => clashes[key] ?? 0

  // -- revisions ----------------------------------------------------------

  snapshot = (key: text, who: text) => {
    let p = pads[key]
    if p != none {
      let kept = revisions[key] ?? []
      let cap = limits["pad"]!["revisions"]!
      let all = [...kept, hold_rev(key, p!.body, who)]
      revisions = put(revisions, key, if len(all) > cap { slice(all, len(all) - cap, len(all)) } else { all })
    }
  }

  hold_rev = (key: text, body: text, who: text) => {
    return { ref: id(), pad: key, body: body, at: now(), by: who }
  }

  revisionsOn = (key: text) => {
    let kept = revisions[key] ?? []
    return sort(kept, (r: slate.data.Revision) => 0 - r.at)
  }

  // Restoring is an edit like any other, so it merges like one and the
  // people typing at that moment keep their lines.
  restore = (key: text, ref: text, who: text) => {
    let found = find(revisions[key] ?? [], (r: slate.data.Revision) => r.ref == ref)
    if found == none {
      return "no such revision"
    }
    return commit(key, bodyOf(key), found!.body, who)
  }

  // -- presence -----------------------------------------------------------
  //
  // Anonymous, but not nameless: a pad where everyone is "someone" is
  // unusable. Names come off a fixed list, so they are stable for a
  // session and mean nothing afterwards.

  nextName = () => {
    seq = seq + 1
    let stones = ["granite", "basalt", "marble", "flint", "quartz", "onyx", "chalk", "sandstone"]
    return stones[seq % len(stones)]! + " " + text(seq)
  }

  arrive = (key: text, who: text, name: text) => {
    here = put(here, key, put(here[key] ?? {}, who, name))
  }

  depart = (key: text, who: text) => {
    here = put(here, key, drop(here[key] ?? {}, who))
  }

  peopleOn = (key: text) => map(keys(here[key] ?? {}), (w: text) => (here[key] ?? {})[w] ?? "")

  // The caret's offset counts characters; the line counts the newlines
  // before it, which is what the merge cares about.
  lineOf = (body: text, at: number) => len(split(slice(body, 0, at), "\n")) - 1

  markCaret = (key: text, who: text, at: number) => {
    let onPad = carets[key] ?? {}
    carets = put(carets, key, put(onPad, who, at))
    publish("slate.carets." + key, at)
  }

  caretsOn = (key: text) => carets[key] ?? {}

  // Everyone whose caret sits on the same line as `who`, excluding them.
  sharingLine = (key: text, who: text) => {
    let all = carets[key] ?? {}
    let mine = all[who] ?? 0 - 1
    return filter(keys(all), (w: text) => w != who and all[w] ?? 0 - 1 == mine)
  }

  dropCaret = (key: text, who: text) => {
    carets = put(carets, key, drop(carets[key] ?? {}, who))
  }

  countOn = (key: text) => len(keys(here[key] ?? {}))
}
