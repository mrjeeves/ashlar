space srv

part Server {
  state ready: bool = false
  start stack = () => {
    return { redy: true }
  }
}
