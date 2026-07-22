space chat.data

part Message {
  id: text
  body: text
}

part Store {
  stored messages: {chat.data.Message} = {}
}
