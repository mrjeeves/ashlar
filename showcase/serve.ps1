#!/usr/bin/env pwsh
# showcase/serve.ps1 — the PowerShell twin of serve.sh: run every example at
# once, each on its own port. The gallery (port 8080) is itself an Ashlar
# program and frames the others, learning their addresses from
# examples/gallery/settings.json. Ctrl-C stops them all. Works on Windows
# PowerShell 5.1, and on pwsh anywhere.
#
# The port override is `ashlar run --port N`: the source keeps `port = 8080`,
# and where it actually serves is a deployment fact bound here (B5). Nothing in
# any example changes.

$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

$onWindows = $env:OS -eq 'Windows_NT'
$bin = if ($onWindows) { 'target\release\ashlar.exe' } else { 'target/release/ashlar' }

# Build, rather than build-only-if-missing: `cargo build` is the incremental
# build system and a no-op when nothing changed, while the old check served
# whatever binary was lying around — so a `git pull` left you running the
# code from before it.
if (Get-Command cargo -ErrorAction SilentlyContinue) {
  Write-Host 'building (a no-op if nothing changed)...'
  cargo build --release
  if ($LASTEXITCODE -ne 0) { throw 'build failed' }
} elseif (-not (Test-Path $bin)) {
  throw "no cargo, and no $bin to fall back on. Install Rust 1.65 or newer and run this again."
} else {
  Write-Host "note: cargo is not on PATH, so $bin is used as-is - it may predate this checkout."
}

# name:port — the map serve.sh and examples/gallery/settings.json mirror. Keep
# all three in sync; t_examples_showcase_launchers_agree_on_every_port asserts
# they are.
$examples = @(
  @{ name = 'gallery';    port = 8080 }
  @{ name = 'counter';    port = 8081 }
  @{ name = 'todo';       port = 8082 }
  @{ name = 'poll';       port = 8084 }
  @{ name = 'ticker';     port = 8085 }
  @{ name = 'pong';       port = 8086 }
  @{ name = 'foundry';    port = 8087 }
  @{ name = 'press';      port = 8088 }
  @{ name = 'guardrails'; port = 8089 }
  @{ name = 'diary';      port = 8090 }
  @{ name = 'locker';     port = 8091 }
  @{ name = 'ledger';     port = 8092 }
  @{ name = 'abacus';     port = 8095 }
  @{ name = 'enclave';    port = 8096 }
  @{ name = 'commons';    port = 8093 }
  @{ name = 'hello';      port = 8094 }
  @{ name = 'slate';      port = 8097 }
)

# `ledger` reaches SQLite over the `native` transport, which needs a POSIX
# dynamic loader. On Windows that transport is unavailable by design — the
# cross-platform answer is a worker, which is exactly what `abacus` shows — so
# ledger's page still serves but its store will fault. Everywhere else, build
# the shim first (mirrors the driving test).
if ($onWindows) {
  Write-Host 'note: ledger needs the POSIX-only `native` transport; its store will not load here.'
  Write-Host '      (`abacus` is the cross-platform foreign example - a worker co-process.)'
} elseif (-not (Get-Command rustc -ErrorAction SilentlyContinue)) {
  Write-Host '  ledger needs a Rust toolchain to build its SQLite shim; skipping it.'
} else {
  Write-Host "building ledger's SQLite shim..."
  # Capture the error rather than discarding it: hiding this is what turns a
  # build problem into a mystery `foreign space has no library` later.
  $err = rustc --edition 2021 --crate-name ledger_store --crate-type cdylib `
    -l sqlite3 -o examples/ledger/foreign/ledger.store.so `
    examples/ledger/foreign/ledger.store.rs 2>&1
  if ($LASTEXITCODE -ne 0) {
    Write-Host "  ledger's shim failed to build:"
    $err | ForEach-Object { Write-Host "    $_" }
    if ("$err" -match 'sqlite3') {
      Write-Host ''
      Write-Host '  If that names libsqlite3, install the development package and re-run:'
      Write-Host '    Debian/Ubuntu   sudo apt install libsqlite3-dev'
      Write-Host '    Fedora/RHEL     sudo dnf install sqlite-devel'
      Write-Host '    Arch            sudo pacman -S sqlite'
      Write-Host '    macOS           ships with the Xcode command line tools'
    }
    Write-Host ''
    Write-Host '  The other sixteen examples are unaffected. ledger''s page will serve'
    Write-Host '  but its store will fault at the boundary, with that same correction.'
  } else {
    # Prove reachability rather than assuming the build implies it.
    & $bin foreign check examples/ledger *> $null
    if ($LASTEXITCODE -ne 0) {
      Write-Host '  built, but the capability is still not reachable:'
      & $bin foreign check examples/ledger 2>&1 | ForEach-Object { Write-Host "    $_" }
    }
  }
}

# Two examples reach something this repository does not ship. `abacus` faults
# at its boundary without it, which reads as a broken page rather than a
# missing part; `enclave` serves a roster that is empty for a reason worth
# stating before it is read as "nobody is out there".
if (-not (Get-Command python3 -ErrorAction SilentlyContinue)) {
  Write-Host ''
  Write-Host '  python3 is not on PATH, so `abacus` will serve and then fault at its'
  Write-Host '  boundary — its worker is Python.'
  Write-Host ''
}
if (-not (Test-Path '\\.\pipe\allmystuff-node')) {
  Write-Host ''
  Write-Host '  No mesh node is listening here, so `enclave` will serve with an empty'
  Write-Host '  roster that says why. Install AllMyStuff and it shows the real one —'
  Write-Host '  the mesh THIS machine is on, not a demo of one.'
  Write-Host ''
}

$procs = @()
try {
  Write-Host ''
  # Each example keeps its own log. This used to be discarded, which threw
  # away the only evidence of a program that refused to start — a port
  # already taken, a missing setting, a stored value that no longer fits
  # its shape — while the line below claimed it was up.
  $logs = '.showcase-logs'
  New-Item -ItemType Directory -Force -Path $logs | Out-Null
  foreach ($ex in $examples) {
    $p = Start-Process -FilePath $bin `
      -ArgumentList @('run', "examples/$($ex.name)", '--port', "$($ex.port)") `
      -PassThru -WindowStyle Hidden `
      -RedirectStandardOutput (Join-Path $logs "$($ex.name).log") `
      -RedirectStandardError (Join-Path $logs "$($ex.name).err")
    $procs += $p
    Write-Host ("  {0,-12} http://127.0.0.1:{1}" -f $ex.name, $ex.port)
  }

  # Starting a process says nothing about whether it stayed started, so ask
  # before claiming.
  Start-Sleep -Seconds 1
  $dead = @()
  for ($i = 0; $i -lt $procs.Count; $i++) {
    if ($procs[$i].HasExited) { $dead += $examples[$i].name }
  }

  Write-Host ''
  if ($dead.Count -eq 0) {
    Write-Host ("All {0} are up. Open the gallery:" -f $procs.Count)
    Write-Host ''
    Write-Host '  http://127.0.0.1:8080'
  } else {
    Write-Host ("{0} of {1} did not start: {2}" -f $dead.Count, $procs.Count, ($dead -join ', '))
    foreach ($name in $dead) {
      Write-Host ''
      Write-Host "  $name said:"
      foreach ($f in @((Join-Path $logs "$name.err"), (Join-Path $logs "$name.log"))) {
        if (Test-Path $f) {
          Get-Content $f -Tail 6 | ForEach-Object { Write-Host "    $_" }
        }
      }
    }
    Write-Host ''
    Write-Host "  Full output for every example is in $logs\."
    if ($dead -contains 'gallery') {
      Write-Host '  The gallery is the page on 8080, so that address will refuse to connect.'
    } else {
      Write-Host ''
      Write-Host '  The rest are up. Open the gallery:'
      Write-Host ''
      Write-Host '    http://127.0.0.1:8080'
    }
  }
  Write-Host ''
  Write-Host 'Press Ctrl-C to stop them all.'

  while ($true) { Start-Sleep -Seconds 1 }
}
finally {
  Write-Host ''
  Write-Host 'stopping...'
  foreach ($p in $procs) {
    if ($p -and -not $p.HasExited) {
      Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    }
  }
}
