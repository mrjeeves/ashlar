"""The whole worker: read a JSON object per line, answer with one.

This is the entire foreign contract (§9.10, ADR-0017) — no shared library, no
C ABI, no build step. Ashlar sends {"call": name, "args": [...]}; an answer is
{"ok": value}, {"error": text}, or a bare value. `flush=True` matters: the
runtime is waiting on a line.
"""

import json
import statistics
import sys


def summarize(entry):
    numbers = []
    for token in entry.replace(",", " ").split():
        try:
            numbers.append(float(token))
        except ValueError:
            pass
    if not numbers:
        return {"mean": 0, "median": 0, "spread": 0}
    return {
        "mean": round(statistics.mean(numbers), 4),
        "median": round(statistics.median(numbers), 4),
        "spread": round(statistics.pstdev(numbers), 4),
    }


CALLS = {"summarize": summarize}

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
        answer = {"ok": call(*request.get("args", []))}
    print(json.dumps(answer), flush=True)
