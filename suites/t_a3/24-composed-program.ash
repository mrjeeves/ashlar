space chat.data

part Message {
  id: text
  body: text
}

part Store {
  stored messages: {text: chat.data.Message} = {}
  add = (m: chat.data.Message) => {
    messages = put(messages, m.id, m)
  }
}

// file: b.ash
space chat.audit
use chat.data

part chat.data.Store {
  add = (m: chat.data.Message) => {
    log.info("adding", { id: m.id })
    messages = put(messages, m.id, m)
  }
}

// file: c.ash
space chat.api
use chat.audit

part messages {
  route = "/api/messages"
  handle pipe = (req: std.Request) => {
    chat.data.Store.add({ id: id(), body: "hello" })
    return chat.data.Store.messages
  }
}
