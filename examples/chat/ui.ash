space chat.ui
use chat.api

part page {
  route = "/"
  view = () => el(room, {})
}

// The whole interface is two view parts. `room` owns the compose form's
// per-instance state; `feed` reads the store, so every post — from this
// client, another client, or the HTTP API — re-renders it live (§9.3).
part room {
  state author: text = ""
  state draft: text = ""
  view = () => el("div", {}, [
    el("h2", {}, ["ashlar chat"]),
    el("p", {}, ["messages: " + text(len(chat.data.Store.messages))]),
    el(feed, {}),
    el("form", { onsubmit: send }, [
      el("input", { oninput: named, value: author, placeholder: "name" }, []),
      el("input", { oninput: typed, value: draft, placeholder: "say something" }, []),
      el("button", {}, ["send"]),
    ]),
  ])
  named = (e: std.Event) => {
    author = text(e.data.value)
  }
  typed = (e: std.Event) => {
    draft = text(e.data.value)
  }
  send = () => {
    chat.data.Store.add({
      id: id(),
      author: if author != "" { author } else { "anon" },
      body: chat.data.Store.prepare(draft),
      sent: now(),
    })
    draft = ""
  }
}

part feed {
  view = () => el("div", {}, rows())
  rows = () => map(ordered(), (m: chat.data.Message) => el("p", {}, [m.author + ": " + m.body]))
  ordered = () => {
    let msgs = chat.data.Store.messages
    return sort(map(keys(msgs), (k: text) => msgs[k]!), (m: chat.data.Message) => m.sent)
  }
}
