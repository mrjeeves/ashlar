space chat.data

part Message {
  id: text
  author: text
  body: text
  sent: number
}

part Store {
  stored messages: {text: chat.data.Message} = {}
  add = (m: chat.data.Message) => {
    messages = put(messages, m.id, m)
  }
  prepare pipe = (body: text) => slice(body, 0, 200)
}
