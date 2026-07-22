space config

part Config {
  tags append: [text] = ["core"]
}

// file: b.ash
space config.ext
use config

part config.Config {
  tags append = ["extra"]
}
