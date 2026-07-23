space chat.ui
use chat.api

part page {
  route = "/"
  view = () => el(feed, {})
}

part feed {
  view = () => el("div", {}, ["messages: " + text(len(chat.data.Store.messages))])
}
