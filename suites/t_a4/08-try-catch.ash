space demo

part Loader {
  load = () => {
    try {
      fail(500, "boom")
    } catch e {
      log.error("load failed")
    }
  }
}
