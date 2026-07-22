space chat.data

part builder {
  extend = (base: [text], extra: text) => [...base, extra]
  merge = (base: {text}, patch: {text}) => { return { ...base, ...patch } }
}
