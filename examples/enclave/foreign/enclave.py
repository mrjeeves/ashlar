"""A stand-in for the mesh, so this example runs on a machine that has none.

It speaks the whole `mesh` and `mesh.sites` contract (§9.10) over JSON Lines
and answers truthfully about a mesh of one: this node, and whichever peers
`foreign/peers.json` says are on it. There are none unless something wrote
that file, so the shipped example shows the empty roster rather than a
pretend one.

The real binding is the mesh daemon this machine runs, which is what the two
space names derive to with no `foreign.json` at all. Delete this file and its
binding and the same program talks to the real thing — that is the whole
point of a capability whose transport is a deployment fact (ADR-0017).
"""

import json
import os
import sys

STATE = {"network": "", "label": "", "revision": 0, "stamp": None, "exposed": {}}
PEERS_FILE = os.path.join("foreign", "peers.json")


def _read_peers():
    """The peers this stand-in has been told about, and the file's stamp.

    A mesh daemon watches the network; a stand-in watches a file. Both answer
    the same question, and the stamp is what makes `revision` mean something.
    """
    try:
        stat = os.stat(PEERS_FILE)
    except OSError:
        return [], None
    stamp = (stat.st_mtime_ns, stat.st_size)
    try:
        with open(PEERS_FILE, encoding="utf-8") as f:
            peers = json.load(f)
    except (OSError, ValueError):
        return [], stamp
    return (peers if isinstance(peers, list) else []), stamp


def _peer_rows():
    peers, _ = _read_peers()
    rows = []
    for p in peers:
        rows.append(
            {
                "id": str(p.get("id", "")),
                "label": str(p.get("label", "")),
                "here": bool(p.get("here", False)),
            }
        )
    return rows


def _bump_if_changed():
    _, stamp = _read_peers()
    if stamp != STATE["stamp"]:
        STATE["stamp"] = stamp
        STATE["revision"] += 1
    return STATE["revision"]


def here():
    return {
        "id": "local",
        "label": STATE["label"] or "this node",
        "network": STATE["network"] or "none",
        "peers": len(_peer_rows()),
    }


def peers():
    return _peer_rows()


def enter(network, label):
    STATE["network"] = network
    STATE["label"] = label
    return here()


def revision():
    return _bump_if_changed()


def reread():
    return STATE["revision"]


def expose(port, label, network):
    """`ashlar run --mesh` calling in: put the port this origin serves on the
    mesh. A stand-in has no proxy to offer peers, so it records the site and
    says so truthfully — the node is local and the mesh is whichever one was
    asked for."""
    STATE["exposed"][int(port)] = label
    if network:
        STATE["network"] = network
    return {"node": "local", "network": STATE["network"] or "none", "label": label}


def unexpose(port):
    STATE["exposed"].pop(int(port), None)
    return True


def published():
    return [
        {"peer": "local", "label": label, "url": ""}
        for _, label in sorted(STATE["exposed"].items())
    ]


def nearby():
    rows = []
    peers_raw, _ = _read_peers()
    for p in peers_raw:
        for s in p.get("sites") or []:
            rows.append(
                {
                    "peer": str(p.get("label", "")),
                    "label": str(s.get("label", "")),
                    "url": str(s.get("url", "")),
                }
            )
    return rows


CALLS = {
    "here": here,
    "peers": peers,
    "enter": enter,
    "revision": revision,
    "reread": reread,
    "expose": expose,
    "unexpose": unexpose,
    "published": published,
    "nearby": nearby,
}

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        msg = json.loads(line)
        fn = CALLS[msg["call"]]
        answer = {"ok": fn(*msg.get("args", []))}
    except KeyError as e:
        answer = {"error": "no such call: %s" % e}
    except Exception as e:  # a fault crosses as a message, never a crash
        answer = {"error": str(e)}
    print(json.dumps(answer), flush=True)
