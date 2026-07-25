space store

part Row {
  key: text
}

foreign save: (key: text) -> bool writes Row
foreign all: () -> [Row] reads Row
