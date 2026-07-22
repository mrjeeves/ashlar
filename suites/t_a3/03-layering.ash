space chat.data

part Message {
  id: text
  body: text
}

// file: b.ash
space chat.audit
use chat.data

part chat.data.Message {
  audit: text = "none"
}
