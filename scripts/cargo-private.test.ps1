$ErrorActionPreference='Stop'
$savedEncoded=$env:CARGO_ENCODED_RUSTFLAGS
$savedPlain=$env:RUSTFLAGS
function global:cargo {
 param([Parameter(ValueFromRemainingArguments=$true)][string[]]$Arguments)
 $global:naifrogPrivacyTestFlags=$env:CARGO_ENCODED_RUSTFLAGS
 $global:LASTEXITCODE=if ($Arguments -contains 'fail') { 7 } else { 0 }
}
try {
 foreach ($encoded in @('',('--cfg'+[char]31+'privacy_probe'))) {
  $env:CARGO_ENCODED_RUSTFLAGS=$encoded
  $env:RUSTFLAGS='-C debuginfo=0'
  & "$PSScriptRoot/cargo-private.ps1" -CargoArgs @('build')
  $captured=@($global:naifrogPrivacyTestFlags -split [char]31)
  $maps=@($captured | Where-Object { $_.StartsWith('--remap-path-prefix=') })
  if ($maps.Count -lt 4 -or @($maps | Where-Object { ([regex]::Matches($_,'--remap-path-prefix=')).Count -ne 1 }).Count) { throw 'Remapping arguments were concatenated or missing' }
  $projectDirectory=Split-Path -Parent $PSScriptRoot
  if ($maps -notcontains "--remap-path-prefix=$projectDirectory=/project") { throw 'Project prefix was not remapped' }
  $expected=if ($encoded) { @('--cfg','privacy_probe') } else { @('-C','debuginfo=0') }
  if ($captured[0] -ne $expected[0] -or $captured[1] -ne $expected[1]) { throw 'Existing compiler flags were not preserved' }
  if ($env:CARGO_ENCODED_RUSTFLAGS -ne $encoded -and -not ([string]::IsNullOrEmpty($env:CARGO_ENCODED_RUSTFLAGS) -and [string]::IsNullOrEmpty($encoded))) { throw 'Compiler environment was not restored' }
  $caught=$false
  try { & "$PSScriptRoot/cargo-private.ps1" -CargoArgs @('fail') } catch { $caught=$true }
  if (-not $caught -or $env:CARGO_ENCODED_RUSTFLAGS -ne $encoded) { if (-not ($caught -and [string]::IsNullOrEmpty($encoded) -and [string]::IsNullOrEmpty($env:CARGO_ENCODED_RUSTFLAGS))) { throw 'Failed builds did not restore the environment' } }
 }
 Write-Output 'PASS: remapping argument boundaries, existing flags, and restoration on success/failure'
} finally {
 $env:CARGO_ENCODED_RUSTFLAGS=$savedEncoded
 $env:RUSTFLAGS=$savedPlain
 Remove-Item -LiteralPath Function:/cargo
}
exit 0
