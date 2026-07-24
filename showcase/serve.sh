#!/usr/bin/env bash
# showcase/serve.sh — run every example at once, each on its own port, so
# showcase/index.html can iframe them side by side. Ctrl-C stops them all.
#
# The port override is `ashlar run --port N`: the source keeps `port = 8080`,
# and where it actually serves is a deployment fact bound here (B5). Nothing
# in any example changes.
set -u

cd "$(dirname "$0")/.."
BIN=target/release/ashlar

if [ ! -x "$BIN" ]; then
  echo "building the release binary first…"
  cargo build --release || { echo "build failed"; exit 1; }
fi

# name:port — the map index.html mirrors. Keep the two in sync.
EXAMPLES=(
  "counter:8081"
  "todo:8082"
  "chat:8083"
  "poll:8084"
  "ticker:8085"
  "pong:8086"
  "foundry:8087"
  "press:8088"
  "guardrails:8089"
  "diary:8090"
  "locker:8091"
  "ledger:8092"
  "commons:8093"
  "hello:8094"
)

# ledger reads a real SQLite database across the foreign boundary; its shim
# must be built before the server can load it (mirrors the driving test).
if command -v rustc >/dev/null 2>&1; then
  echo "building ledger's SQLite shim…"
  rustc --edition 2021 --crate-name ledger_store --crate-type cdylib \
    -l sqlite3 -o examples/ledger/foreign/ledger.store.so \
    examples/ledger/foreign/ledger.store.rs 2>/dev/null \
    || echo "  (skipped: needs a Rust toolchain + libsqlite3 — ledger's frame will be empty)"
fi

PIDS=()
cleanup() {
  echo
  echo "stopping…"
  for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null; done
  wait 2>/dev/null
  exit 0
}
trap cleanup INT TERM

echo
for entry in "${EXAMPLES[@]}"; do
  name="${entry%%:*}"
  port="${entry##*:}"
  "$BIN" run "examples/$name" --port "$port" >/dev/null 2>&1 &
  PIDS+=($!)
  printf '  %-12s http://127.0.0.1:%s\n' "$name" "$port"
done

echo
echo "All examples are up. Open showcase/index.html in a browser."
echo "Press Ctrl-C to stop them all."
wait
