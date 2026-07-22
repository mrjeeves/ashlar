space demo

part Counter {
  state n: number = 0
  run = () => {
    while n < 10 {
      n = n + 1
    }
  }
}
