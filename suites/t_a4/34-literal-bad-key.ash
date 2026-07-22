space demo

part Message {
  id: text
  body: text
}

part W {
  save = (m: Message) => m
  go = () => save({ id: "1", bod: "hi", body: "x" })
}
