"""The same three operations as ledger.store.so, over the `worker` transport.

The capability is named once, in `data.ash`. HOW it is reached is a deployment
fact (ADR-0017), and this file is the second answer to that question: the same
SQL, the same shapes, no compiler and no development package — Python's
standard library ships SQLite.

The C-ABI shim next to this file is the default on a machine with a POSIX
loader, because `native` is the transport it exists to defend. Where that
cannot work — Windows, which has no `dlopen`; a machine with no Rust toolchain;
one where `-l sqlite3` finds no development package — the launcher binds the
space here instead, and the example runs unchanged. Neither the Ashlar source
nor the shape checks know which one answered.

Protocol (§9.10): read {"call": name, "args": [...]} per line, answer with
{"ok": value}, {"error": text}, or a bare value. `flush=True` matters: the
runtime is waiting on a line.

The database location is a deployment fact too (B5): an argv item on the
binding if there is one, else `ASHLAR_LEDGER_DB`, else a temp file named for
the SERVER's process — the same file the shim would pick, because the shim runs
inside that process and this runs beside it. So the two bindings are
interchangeable at run time and not only in principle.
"""

import json
import os
import sqlite3
import sys
import tempfile


def db():
    path = sys.argv[1] if len(sys.argv) > 1 else os.environ.get("ASHLAR_LEDGER_DB")
    if not path:
        path = os.path.join(tempfile.gettempdir(), "ashlar-ledger-%d.db" % os.getppid())
    conn = sqlite3.connect(path)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS entries("
        "id INTEGER PRIMARY KEY AUTOINCREMENT,"
        "who TEXT NOT NULL, note TEXT NOT NULL, amount REAL NOT NULL)"
    )
    return conn


def record(who, note, amount):
    conn = db()
    with conn:
        conn.execute(
            "INSERT INTO entries(who,note,amount) VALUES(?,?,?)", (who, note, amount)
        )
    conn.close()
    return True


def recent():
    conn = db()
    rows = conn.execute(
        "SELECT who,note,amount FROM entries ORDER BY id DESC"
    ).fetchall()
    conn.close()
    # Shaped to fit `[Entry]`, and checked against it at the boundary — a
    # column that stopped matching faults at the call site rather than
    # slipping through as bad data.
    return [{"who": w, "note": n, "amount": a} for (w, n, a) in rows]


def total():
    conn = db()
    (sum_,) = conn.execute("SELECT coalesce(sum(amount),0) FROM entries").fetchone()
    conn.close()
    return sum_


CALLS = {"record": record, "recent": recent, "total": total}

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    request = json.loads(line)
    call = CALLS.get(request.get("call"))
    if call is None:
        # An unknown call is an error, not a crash — which is also what lets
        # `ashlar foreign check` prove this worker speaks the protocol.
        answer = {"error": "no such foreign function: " + str(request.get("call"))}
    else:
        try:
            answer = {"ok": call(*request.get("args", []))}
        except Exception as e:  # a failed query is an answer, not a dead worker
            answer = {"error": "%s: %s" % (type(e).__name__, e)}
    print(json.dumps(answer), flush=True)
