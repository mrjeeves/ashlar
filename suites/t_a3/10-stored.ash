space chat.data

part Message {
  id: text
  body: text
}

part Store {
  stored messages: {text: chat.data.Message} = {}
}
