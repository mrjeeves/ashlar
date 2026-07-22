space chat.data

part Message {
  body: text
}

// file: b.ash
space chat.ui
use chat.data

part Feed {
  latest: Message?
}
