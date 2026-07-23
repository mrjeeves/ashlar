space chat.data

// A data shape: every field typed, no behavior. Values are checked
// against it wherever one is constructed (§5).
part Message {
  id: text
  author: text
  body: text
  sent: number
}

// `stored` persists through restarts (§9.3). `prepare` is a pipe, so
// other spaces can stack their own layers on it — chat.audit does.
part Store {
  stored messages: {text: chat.data.Message} = {}
  add = (m: chat.data.Message) => {
    messages = put(messages, m.id, m)
  }
  prepare pipe = (body: text) => slice(body, 0, 200)
}
