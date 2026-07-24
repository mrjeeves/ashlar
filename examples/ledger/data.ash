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
//
// `reads Entry` / `writes Entry` make the foreign store REACTIVE (§9.3):
// the collection is the table, the Shape is the schema. A view that calls
// `recent` or `total` depends on the Entry collection; `record` writing it
// re-renders every such view and patches it — across every connected
// client — so the SQL store goes live without leaving the loop (ADR-0014).
foreign record: (who: text, note: text, amount: number) -> bool writes Entry
foreign recent: () -> [Entry] reads Entry
foreign total: () -> number reads Entry
