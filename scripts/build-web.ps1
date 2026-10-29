param([switch]$Release)
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
 if (!(Test-Path './assets/processed/game-assets.json')) {
  # The asset pipeline encodes WebP with sharp (package.json).
  if (!(Test-Path './node_modules/sharp')) { npm ci; if ($LASTEXITCODE -ne 0) { throw 'npm ci failed' } }
  node scripts/prepare-game-assets.mjs; if ($LASTEXITCODE -ne 0) { throw 'Asset preparation failed' }
 }
 rustup target add wasm32-unknown-unknown
 if ($LASTEXITCODE -ne 0) { throw 'WASM target installation failed' }
 $lock=Get-Content './Cargo.lock' -Raw
 $version=[regex]::Match($lock,'(?ms)name = "wasm-bindgen"\s+version = "([^"]+)"').Groups[1].Value
 if (!$version) { throw 'Missing wasm-bindgen in Cargo.lock; run cargo fetch first' }
 $installed=if (Get-Command wasm-bindgen -ErrorAction SilentlyContinue) { wasm-bindgen --version } else { '' }
 if ($installed -ne "wasm-bindgen $version") { cargo install wasm-bindgen-cli --version $version --locked; if ($LASTEXITCODE -ne 0) { throw 'Bindings generator installation failed' } }
 $profile='debug';$cargoArgs=@('build','--locked','--target','wasm32-unknown-unknown','--bin','naifrog-duel')
 # web-release (Cargo.toml): fat LTO in one codegen unit, slower to build but a smaller download.
 if ($Release) { $profile='web-release';$cargoArgs+=@('--profile','web-release') }
 & "$PSScriptRoot/cargo-private.ps1" -CargoArgs $cargoArgs
 if ($LASTEXITCODE -ne 0) { throw 'WASM compilation failed' }
 $bindgenArgs=@("--target","web","--no-typescript","--out-name","naifrog_duel","--out-dir","web/pkg")
 if ($Release) { $bindgenArgs+=@("--remove-name-section","--remove-producers-section") }
 wasm-bindgen @bindgenArgs "target/wasm32-unknown-unknown/$profile/naifrog-duel.wasm"
 if ($LASTEXITCODE -ne 0) { throw 'Bindings generation failed' }
 # naifrog-server sends this copy to browsers that accept gzip: about a fifth of the bytes.
 node -e "const z=require('node:zlib'),fs=require('node:fs'),f='web/pkg/naifrog_duel_bg.wasm';fs.writeFileSync(f+'.gz',z.gzipSync(fs.readFileSync(f),{level:9}))"
 if ($LASTEXITCODE -ne 0) { throw 'WASM compression failed' }
 Write-Host 'Web build ready. Public deployment: naifrog-server, behind HTTPS or on http://IP:port without a domain; see DEPLOY.md. The Node static server is offline debugging only.'
} finally { Pop-Location }
