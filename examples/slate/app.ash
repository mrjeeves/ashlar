space slate.app
use slate.ui

part app {
  port = 8080
  style = "slate"
  start stack = () => {
    slate.data.Store.boot()
    slate.data.Store.ensurePad("welcome", "welcome to slate")
    return none
  }
  stop stack reverse = () => {
    slate.data.Store.wind()
    return none
  }
}

part home {
  route = "/"
  view = () => el(slate.ui.index, {})
}

// A page's identity is the page, not a person: every open tab gets a name
// off the list so the roster can say who is here, and it means nothing
// once the socket closes. That is the whole of "identity" in this
// program, and it is why there is nothing to log into.
part padPage {
  route = "/p/{key}"
  handle pipe = (req: std.Request) => {
    let key = req.params["key"]!
    slate.data.Store.ensurePad(key, key)
    return el(slate.ui.editor, { key: key, me: slate.data.Store.nextName() })
  }
}

// A native form post, so making a pad works with no client code at all.
part make {
  route = "/new"
  handle pipe = (req: std.Request) => {
    // `fields` is the one question a boundary needs and could not ask:
    // everything from outside is `data`, a union, and this answers `none`
    // for every member of it that is not a map. One guard covers a missing
    // body, a body that is not JSON, and a body that is JSON but not an
    // object — and the refusal is the caller's 400, not the server's 500.
    let form = fields(req.data)
    if form == none {
      return fail(400, "a title, please")
    }
    let title = text(form!["title"] ?? "")
    let key = slug(if title != "" { title } else { "pad" })
    slate.data.Store.ensurePad(key, if title != "" { title } else { key })
    return redirect("/p/" + key)
  }

  // A URL is a name someone has to type to a colleague, so it gets the
  // same discipline the language applies to its own names: lower case,
  // one separator, nothing surprising.
  slug = (title: text) => {
    let flat = join(filter(split(lower(title), " "), (w: text) => w != ""), "-")
    let cut = slice(flat, 0, 40)
    return if cut != "" { cut } else { "pad" }
  }

  lower = (t: text) => join(map(split(t, ""), (c: text) => down(c)), "")

  down = (c: text) => {
    let at = firstAt("ABCDEFGHIJKLMNOPQRSTUVWXYZ", c)
    return if at >= 0 { slice("abcdefghijklmnopqrstuvwxyz", at, at + 1) } else { c }
  }

  firstAt = (haystack: text, needle: text) => {
    return spot(haystack, needle, 0)
  }

  spot = (haystack: text, needle: text, at: number) => {
    if at >= len(haystack) {
      return 0 - 1
    }
    return if slice(haystack, at, at + 1) == needle { at } else { spot(haystack, needle, at + 1) }
  }
}

// The pad as data: what a script, a backup, or another program reads.
part padApi {
  route = "/api/pad/{key}"
  handle pipe = (req: std.Request) => {
    let key = req.params["key"]!
    if slate.data.Store.pads[key] == none {
      return fail(404, "no such pad")
    }
    return {
      key: key,
      title: slate.data.Store.titleOf(key),
      body: slate.data.Store.bodyOf(key),
      here: slate.data.Store.peopleOn(key),
      clashes: slate.data.Store.clashesOn(key),
      revisions: len(slate.data.Store.revisionsOn(key)),
      policies: slate.data.Store.tags,
    }
  }
}

// Writing over HTTP takes the same path a keystroke does — `base` and
// all, because a writer that will not say what it was editing cannot be
// merged, only obeyed. A client that has not read the pad sends the empty
// base and is treated as someone typing into a blank page, which is what
// they are.
part editApi {
  route = "/api/edit/{key}"
  handle pipe = (req: std.Request) => {
    let edit = fields(req.data)
    if edit == none {
      return fail(400, "an edit is a JSON object: { base, body, who }")
    }
    let e = edit!
    let key = req.params["key"]!
    if slate.data.Store.pads[key] == none {
      return fail(404, "no such pad")
    }
    // An absent `body` is not an empty pad, and treating it as one would
    // answer a malformed request with a cheerful 200 for work that never
    // happened. `data` keeps the difference between "" and missing;
    // asking is the whole cost of not lying to the caller.
    if e["body"] == none {
      return fail(400, "`body` is the pad's new text; an edit without one is not an edit")
    }
    let why = slate.data.Store.commit(key, text(e["base"] ?? ""), text(e["body"] ?? ""), text(e["who"] ?? "a script"))
    if why != "" {
      return fail(409, why)
    }
    return { key: key, body: slate.data.Store.bodyOf(key), clashes: slate.data.Store.clashesOn(key) }
  }
}
