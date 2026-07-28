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

# Build, rather than build-only-if-missing. `cargo build` IS the incremental
# build system — it is a no-op in a fraction of a second when nothing
# changed — and the old check ran whatever binary happened to be lying
# around, so a `git pull` left you serving the code from before it. For a
# showcase whose whole claim is "the frames are the real servers", running
# yesterday's compiler against today's examples is the one failure that
# looks like success.
if command -v cargo >/dev/null 2>&1; then
  echo "building (a no-op if nothing changed)…"
  cargo build --release || { echo "build failed"; exit 1; }
elif [ ! -x "$BIN" ]; then
  echo "no cargo, and no $BIN to fall back on."
  echo "Install Rust 1.65 or newer and run this again."
  exit 1
else
  echo "note: cargo is not on PATH, so $BIN is used as-is —"
  echo "      it may predate this checkout."
fi

# name:port — the map examples/gallery/settings.json mirrors for the seventeen
# it frames. Keep them in sync; t_examples_showcase_launchers_agree_on_every_port
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
  "enclave:8096"
  "commons:8093"
  "hello:8094"
  "slate:8097"
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
  echo "  The other seventeen examples are unaffected. ledger's page will serve"
  echo "  but its store will fault at the boundary, with that same correction."
  echo "  \`abacus\` is the foreign example that needs no compiler at all."
  echo
fi

# Two examples reach a co-process rather than a library: `abacus` runs a Python
# worker, and `enclave` ships a stand-in for the mesh so it demonstrates on a
# machine that has none. Both serve without python3 and then fault at the
# boundary on every call — which reads as a broken page rather than a missing
# package, so say it here instead.
if ! command -v python3 >/dev/null 2>&1; then
  echo
  echo "  python3 is not on PATH. \`abacus\` and \`enclave\` will serve, but every"
  echo "  call across their boundary will fault with that correction — their"
  echo "  co-processes are Python. The other sixteen are unaffected."
  echo "  (\`enclave\` also runs against a real mesh: delete its foreign.json and"
  echo "  it binds the mesh node this machine runs.)"
  echo
fi

PIDS=()
NAMES=()
LOGS=".showcase-logs"
cleanup() {
  echo
  echo "stopping…"
  for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null; done
  wait 2>/dev/null
  exit 0
}
trap cleanup INT TERM

# Each example keeps its own log. This used to be `>/dev/null 2>&1`, which
# threw away the only evidence of a program that refused to start — a
# missing setting, a port already taken, a stored value that no longer fits
# its shape. The line below printed anyway, so the launcher said a server
# was at an address where nothing was listening.
mkdir -p "$LOGS"

echo
for entry in "${EXAMPLES[@]}"; do
  name="${entry%%:*}"
  port="${entry##*:}"
  "$BIN" run "examples/$name" --port "$port" > "$LOGS/$name.log" 2>&1 &
  PIDS+=($!)
  NAMES+=("$name")
  printf '  %-12s http://127.0.0.1:%s\n' "$name" "$port"
done

# Starting a background process says nothing about whether it stayed
# started, so ask before claiming. A launcher that reports success it did
# not check is worse than one that reports nothing.
sleep 1
DEAD=()
for i in "${!PIDS[@]}"; do
  if ! kill -0 "${PIDS[$i]}" 2>/dev/null; then
    DEAD+=("${NAMES[$i]}")
  fi
done

echo
if [ ${#DEAD[@]} -eq 0 ]; then
  echo "All ${#PIDS[@]} are up. Open the gallery:"
  echo
  echo "  http://127.0.0.1:8080"
else
  echo "${#DEAD[@]} of ${#PIDS[@]} did not start: ${DEAD[*]}"
  for name in "${DEAD[@]}"; do
    echo
    echo "  $name said:"
    tail -n 6 "$LOGS/$name.log" 2>/dev/null | sed 's/^/    /'
  done
  echo
  echo "  Full output for every example is in $LOGS/."
  if printf '%s\n' "${DEAD[@]}" | grep -qx gallery; then
    echo "  The gallery is the page on 8080, so that address will refuse to connect."
  else
    echo
    echo "  The rest are up. Open the gallery:"
    echo
    echo "    http://127.0.0.1:8080"
  fi
fi
echo
echo "Press Ctrl-C to stop them all."
wait
