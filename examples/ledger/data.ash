space ledger.store

// A ledger entry: a data shape — typed fields, no behavior (§5). The
// foreign boundary shape-checks every row SQLite returns against it, so a
// column that stopped matching would fault at the call site, not slip
// through as bad data.
part Entry {
  who: text
  note: text
  amount: number
}

// The datastore is a real SQLite database file, reached across the
// foreign boundary (§9.10). Ashlar names the operations; the SQL lives in
// foreign/ledger.store — SQL is the persistence peer of CSS, named here
// and defined outside the language (ADR-0014). No connection string or
// query ever appears in source (B5): the shim owns both.
foreign record: (who: text, note: text, amount: number) -> bool
foreign recent: () -> [Entry]
foreign total: () -> number
