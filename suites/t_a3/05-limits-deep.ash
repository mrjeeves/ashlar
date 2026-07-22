space config

part Config {
  limits deep = { http: { max: 10 } }
}

// file: b.ash
space config.ext
use config

part config.Config {
  limits deep = { http: { timeout: 30 } }
}
