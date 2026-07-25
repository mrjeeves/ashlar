space store

part Row {
  key: text
}

foreign save: (key: text) -> bool updates Row
foreign all: () -> [Row] watches Row
