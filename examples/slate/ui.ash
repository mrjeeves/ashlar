space slate.ui
use slate.history

// The editor. The whole client is the runtime's own shim: no editor
// library, no diffing in the browser, no operational-transform code
// shipped to anybody. A keystroke is an `oninput` event, the merge runs
// on the server, and every other page holding this pad is patched with
// the result (§9.4).

part index {
  state draft: text = ""
  view = () => el("div", { class: "app" }, [
    el("title", {}, ["slate"]),
    el("header", { class: "top" }, [
      el("p", { class: "wordmark" }, ["slate"]),
      el("p", { class: "tagline" }, ["a shared pad · the URL is the invitation"]),
    ]),
    el("section", { class: "card" }, [
      el("h1", {}, ["Open a pad"]),
      el("p", { class: "lede" }, ["Anyone with the address can read and write it. There is nothing to sign into."]),
      el("form", { class: "row", action: "/new", method: "post" }, [
        el("input", { class: "field grow", name: "title", oninput: typed, value: draft, placeholder: "what is it about?" }, []),
        el("button", { class: "primary" }, ["make a pad"]),
      ]),
    ]),
    el("section", { class: "card" }, [
      el("h2", {}, ["Pads here"]),
      el("ul", { class: "pads" }, rows()),
    ]),
  ])
  typed = (e: std.Event) => {
    draft = text(e.data.value)
  }
  rows = () => {
    let all = slate.data.Store.padList()
    return if len(all) == 0 { [el("li", { class: "quiet" }, ["none yet — make the first one"])] } else { map(all, (p: slate.data.Pad) => el(slate.ui.padRow, { key: p.key, title: p.title, edits: p.edits })) }
  }
}

part padRow {
  key: text
  title: text
  edits: number
  view = () => el("li", { class: "padrow" }, [
    el("a", { class: "padlink", href: "/p/" + key }, [title]),
    el("span", { class: "count" }, [text(edits) + " edits · " + text(slate.data.Store.countOn(key)) + " here"]),
  ])
}

// One pad, open. `base` is the load-bearing field: it holds the text this
// page had in front of it when its last keystroke left, and it is what
// lets the server merge instead of overwrite. It is per-instance state,
// so every open page has its own — which is exactly right, because every
// page has its own idea of what the pad said a moment ago.
part editor {
  key: text
  me: text
  state base: text = ""
  state warned: [text] = []
  state seen: number = 0

  start stack = () => {
    slate.data.Store.arrive(key, me, me)
    subscribe("slate.clash." + key, heard)
    subscribe("slate.changed." + key, moved)
    subscribe("slate.carets." + key, shifted)
    return none
  }

  stop stack reverse = () => {
    slate.data.Store.depart(key, me)
    return none
  }

  view = () => el("div", { class: "app" }, [
    // The tab is named after the pad, and because a title re-renders like
    // any other element (§9.4), renaming the pad renames the tab.
    el("title", {}, [slate.data.Store.titleOf(key) + " · slate"]),
    el("header", { class: "top" }, [
      el("a", { class: "back", href: "/" }, ["← every pad"]),
      el("h1", { class: "title" }, [slate.data.Store.titleOf(key)]),
      el(slate.ui.roster, { key: key, me: me }),
    ]),
    el("div", { class: "columns" }, [
      el("section", { class: "sheet" }, [
        el("textarea", {
          class: "pad",
          oninput: typed,
          rows: "20",
          spellcheck: "false",
          placeholder: "type here — anyone else on this page sees it as you go",
        }, [slate.data.Store.bodyOf(key)]),
        el("p", { class: "hint" }, [hint()]),
        el("p", { class: "sharing" }, [sharing()]),
      ]),
      el("aside", { class: "side" }, [
        el(slate.ui.notices, { key: key, said: join(warned, "|") }),
        el(slate.ui.history, { key: key, me: me }),
      ]),
    ]),
  ])

  // Every keystroke: hand the server what this page had and what it has
  // now, and adopt whatever the merge decided. Adopting matters — if the
  // pad came back different because someone else's line landed, the next
  // keystroke must be measured against THAT, or this page would keep
  // re-sending a base the pad has moved past.
  typed = (e: std.Event) => {
    let why = slate.data.Store.commit(key, base, text(e.data.value), me)
    base = slate.data.Store.bodyOf(key)
    // Say where this page is working. `caret` is the offset the field
    // already knew (§9.4); the line is what the merge resolves by, so the
    // line is what the pad publishes.
    let at = number(text(e.data.caret)) ?? 0
    slate.data.Store.markCaret(key, me, slate.data.Store.lineOf(text(e.data.value), at))
    if why != "" {
      warned = [...warned, why]
    }
  }

  // Who else is on this line right now. This is the collision the merge
  // would have to resolve, named before it happens rather than reported
  // after — the pad already tells you who LOST a line; this says who is
  // about to be in one with you.
  sharing = () => {
    let with = slate.data.Store.sharingLine(key, me)
    return if len(with) == 0 { "" } else { join(with, ", ") + " " + (if len(with) == 1 { "is" } else { "are" }) + " on your line" }
  }

  // Someone else's caret moved. Nothing to store — the pad holds the
  // carets; this page only needs to re-read them, and touching its own
  // state is what asks for that.
  shifted = (m: data) => {
    seen = seen + 1
  }

  heard = (m: data) => {
    warned = [...warned, text(m)]
  }

  // Somebody's edit landed, and this page is being patched to show it.
  // The base moves with the screen, so the next keystroke here is
  // measured against what is actually in front of this person.
  moved = (m: data) => {
    base = text(m)
  }

  hint = () => {
    let n = slate.data.Store.countOn(key)
    return if n > 1 { text(n) + " people are on this pad. Edits merge line by line; if two of you change one line, the copy already on the pad wins and you are told." } else { "Nobody else is here yet. Send someone the address." }
  }
}

part roster {
  key: text
  me: text
  view = () => el("div", { class: "roster" }, faces())
  faces = () => map(slate.data.Store.peopleOn(key), (who: text) => el("span", { class: cls(who) }, [who]))
  cls = (who: text) => (if who == me { "face self" } else { "face" })
}

// What the pad refused or resolved. `said` is a field rather than read
// state so the tray re-renders when the editor's list changes; the list
// itself is per-page, because a conflict is news for the person whose
// keystroke lost, not for everyone.
part notices {
  key: text
  said: text
  view = () => el("div", { class: "notices" }, items())
  items = () => {
    let all = filter(split(said, "|"), (s: text) => s != "")
    return if len(all) == 0 { [el("p", { class: "quiet" }, ["no conflicts so far"])] } else { map(recent(all), (s: text) => el("p", { class: "notice" }, [s])) }
  }
  recent = (all: [text]) => {
    let n = len(all)
    return if n > 3 { slice(all, n - 3, n) } else { all }
  }
}

part history {
  key: text
  me: text
  view = () => el("div", { class: "history" }, [
    el("h2", {}, ["earlier"]),
    el("ul", { class: "revs" }, rows()),
  ])
  rows = () => {
    let kept = slate.data.Store.revisionsOn(key)
    return if len(kept) == 0 { [el("li", { class: "quiet" }, ["no snapshots yet"])] } else { map(kept, (r: slate.data.Revision) => el(slate.ui.rev, {
      key: key,
      ref: r.ref,
      by: r.by,
      size: len(r.body),
      me: me,
    })) }
  }
}

part rev {
  key: text
  ref: text
  by: text
  size: number
  me: text
  view = () => el("li", { class: "rev" }, [
    el("span", { class: "who" }, [by]),
    el("span", { class: "size" }, [text(size) + " chars"]),
    el("button", { class: "ghost", onclick: put_back }, ["restore"]),
  ])
  // Restoring is an edit, not a rollback: it merges against whatever is
  // on the pad right now, so a person mid-sentence does not lose it.
  put_back = () => {
    slate.data.Store.restore(key, ref, me)
  }
}
