param([Parameter(Mandatory=$true)][string[]]$CargoArgs)
$ErrorActionPreference='Stop'
$projectDirectory=Split-Path -Parent $PSScriptRoot
$profileDirectory=[Environment]::GetFolderPath('UserProfile')
$savedFlags=$env:CARGO_ENCODED_RUSTFLAGS
try {
 [string[]]$flags=@()
 if ($savedFlags) { $flags=@($savedFlags -split [char]31) }
 elseif ($env:RUSTFLAGS) { $flags=@($env:RUSTFLAGS -split '\s+' | Where-Object { $_ }) }
 $cargoDirectory=if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $profileDirectory '.cargo' }
 $rustupDirectory=if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $profileDirectory '.rustup' }
 $prefixes=@(
  @{From=$profileDirectory;To='/user'},
  @{From=$cargoDirectory;To='/cargo-home'},
  @{From=$rustupDirectory;To='/rustup-home'},
  @{From=$projectDirectory;To='/project'}
 )
 foreach ($prefix in $prefixes) {
  foreach ($source in @($prefix.From,$prefix.From.Replace('\','/')) | Select-Object -Unique) {
   $flags+="--remap-path-prefix=$source=$($prefix.To)"
  }
 }
 $env:CARGO_ENCODED_RUSTFLAGS=$flags -join [char]31
 cargo @CargoArgs
 if ($LASTEXITCODE -ne 0) { throw "Cargo failed with exit code $LASTEXITCODE" }
} finally { $env:CARGO_ENCODED_RUSTFLAGS=$savedFlags }
