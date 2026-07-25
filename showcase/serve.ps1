#!/usr/bin/env pwsh
# showcase/serve.ps1 — the PowerShell twin of serve.sh: run every example at
# once, each on its own port, so showcase/index.html can iframe them. Ctrl-C
# stops them all. Works on Windows PowerShell 5.1, and on pwsh anywhere.
#
# The port override is `ashlar run --port N`: the source keeps `port = 8080`,
# and where it actually serves is a deployment fact bound here (B5). Nothing in
# any example changes.

$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

$onWindows = $env:OS -eq 'Windows_NT'
$bin = if ($onWindows) { 'target\release\ashlar.exe' } else { 'target/release/ashlar' }

if (-not (Test-Path $bin)) {
  Write-Host 'building the release binary first...'
  cargo build --release
  if ($LASTEXITCODE -ne 0) { throw 'build failed' }
}

# name:port — the map serve.sh and index.html mirror. Keep all three in sync;
# t_examples_showcase_launchers_agree_on_every_port asserts they do.
$examples = @(
  @{ name = 'counter';    port = 8081 }
  @{ name = 'todo';       port = 8082 }
  @{ name = 'chat';       port = 8083 }
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
  @{ name = 'commons';    port = 8093 }
  @{ name = 'hello';      port = 8094 }
)

# `ledger` reaches SQLite over the `native` transport, which needs a POSIX
# dynamic loader. On Windows that transport is unavailable by design — the
# cross-platform answer is a worker, which is exactly what `abacus` shows — so
# ledger's page still serves but its store will fault. Everywhere else, build
# the shim first (mirrors the driving test).
if ($onWindows) {
  Write-Host 'note: ledger needs the POSIX-only `native` transport; its store will not load here.'
  Write-Host '      (`abacus` is the cross-platform foreign example — a worker co-process.)'
} elseif (Get-Command rustc -ErrorAction SilentlyContinue) {
  Write-Host "building ledger's SQLite shim..."
  rustc --edition 2021 --crate-name ledger_store --crate-type cdylib `
    -l sqlite3 -o examples/ledger/foreign/ledger.store.so `
    examples/ledger/foreign/ledger.store.rs 2>$null
  if ($LASTEXITCODE -ne 0) {
    Write-Host '  (skipped: needs a Rust toolchain + libsqlite3 - ledger''s frame will be empty)'
  }
}

$procs = @()
try {
  Write-Host ''
  foreach ($ex in $examples) {
    $p = Start-Process -FilePath $bin `
      -ArgumentList @('run', "examples/$($ex.name)", '--port', "$($ex.port)") `
      -PassThru -WindowStyle Hidden
    $procs += $p
    Write-Host ("  {0,-12} http://127.0.0.1:{1}" -f $ex.name, $ex.port)
  }

  $page = (Resolve-Path 'showcase/index.html').Path -replace '\\', '/'
  Write-Host ''
  Write-Host 'All fifteen are up. Open the gallery:'
  Write-Host ''
  Write-Host "  file:///$($page -replace '^/', '')"
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
