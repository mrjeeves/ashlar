space chat.data

part builder {
  extend = (base: [text], extra: text) => [...base, extra]
  merge = (base: {text: text}, patch: {text: text}) => { return { ...base, ...patch } }
}
