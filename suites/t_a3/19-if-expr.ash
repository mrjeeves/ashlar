space chat.util

part label {
  describe = (read: bool) => {
    let status = if read { "seen" } else { "new" }
    return status
  }
}
