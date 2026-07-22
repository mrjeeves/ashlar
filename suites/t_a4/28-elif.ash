space demo

part Config {
  classify = (n: number) => {
    if n > 0 {
      return "positive"
    } elif n < 0 {
      return "negative"
    }
  }
}
