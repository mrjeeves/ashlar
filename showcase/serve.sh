#!/usr/bin/env bash
# showcase/serve.sh — run every example at once, each on its own port. The
# gallery (port 8080) is itself an Ashlar program: it frames the others, and it
# learns their addresses from examples/gallery/settings.json rather than from
# anything in its source. Ctrl-C stops them all.
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

# name:port — the map examples/gallery/settings.json mirrors for the fifteen it
# frames. Keep them in sync; t_examples_showcase_launchers_agree_on_every_port
# asserts they are.
EXAMPLES=(
  "gallery:8080"
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
  "abacus:8095"
  "commons:8093"
  "hello:8094"
)

# `ledger` reaches a real SQLite database across the foreign boundary, so its
# shim must be built before the server can load it (mirrors the driving test).
# When this cannot be done, say WHY and what to run — a shrug here just moves
# the confusion to `foreign space 'ledger.store' has no library` later.
build_ledger_shim() {
  if ! command -v rustc >/dev/null 2>&1; then
    echo "  ledger needs a Rust toolchain to build its SQLite shim; skipping it."
    return 1
  fi
  echo "building ledger's SQLite shim…"
  local err
  err="$(rustc --edition 2021 --crate-name ledger_store --crate-type cdylib \
    -l sqlite3 -o examples/ledger/foreign/ledger.store.so \
    examples/ledger/foreign/ledger.store.rs 2>&1)" && return 0

  # Always show what actually went wrong. Hiding this is what turned a build
  # problem into a mystery `foreign space has no library` later.
  echo "  ledger's shim failed to build:"
  printf '%s\n' "$err" | sed 's/^/    /'
  # One cause is common enough to name, since the fix is a package install:
  # linking `-l sqlite3` needs the development package, not just the runtime
  # library most systems already ship.
  if printf '%s' "$err" | grep -qi "sqlite3"; then
    echo
    echo "  If that names libsqlite3, install the development package and re-run:"
    echo "    Debian/Ubuntu   sudo apt install libsqlite3-dev"
    echo "    Fedora/RHEL     sudo dnf install sqlite-devel"
    echo "    Arch            sudo pacman -S sqlite"
    echo "    macOS           ships with the Xcode command line tools"
  fi
  return 1
}

if build_ledger_shim; then
  # Prove it, rather than assuming the build implies reachability — this is
  # exactly what `foreign check` is for.
  if ! "$BIN" foreign check examples/ledger >/dev/null 2>&1; then
    echo "  built, but the capability is still not reachable:"
    "$BIN" foreign check examples/ledger 2>&1 | sed 's/^/    /'
  fi
else
  echo
  echo "  The other fourteen examples are unaffected. ledger's page will serve"
  echo "  but its store will fault at the boundary, with that same correction."
  echo "  \`abacus\` is the foreign example that needs no compiler at all."
  echo
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
echo "All sixteen are up. Open the gallery:"
echo
echo "  http://127.0.0.1:8080"
echo
echo "Press Ctrl-C to stop them all."
wait
