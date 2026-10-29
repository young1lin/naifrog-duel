$ErrorActionPreference='Stop'
if (!$env:NAIFROG_ACCESS_KEY) { throw 'Set NAIFROG_ACCESS_KEY to a private random key of at least 32 bytes before starting.' }
if (!$env:NAIFROG_ASSET_ROOT) { $env:NAIFROG_ASSET_ROOT=$PSScriptRoot }
if (!$env:NAIFROG_BIND) { $env:NAIFROG_BIND='127.0.0.1:18080' }
if (!$env:NAIFROG_ORIGIN) { $env:NAIFROG_ORIGIN='http://127.0.0.1:18080' }
& (Join-Path $PSScriptRoot 'naifrog-server.exe')
exit $LASTEXITCODE
