param(
 # menu: choose in the game. cpu: you are P1 vs the CPU. local: two players. demo: CPU vs CPU.
 [ValidateSet('menu','cpu','local','demo')][string]$Mode='menu',
 [switch]$Ai, [switch]$Local, [switch]$Demo,
 # Private online play: the naifrog-server the browser uses, as http://IP:PORT.
 [string]$Server,
 # Chinese or English; left out, the game follows NAIFROG_LANG, then the system language. F2 switches in game.
 [ValidateSet('zh','en')][string]$Lang
)
$ErrorActionPreference='Stop'
Push-Location (Split-Path -Parent $PSScriptRoot)
try {
 if (!(Test-Path './assets/processed/game-assets.json')) {
  if (!(Test-Path './assets/original/fight-1.png')) { throw 'Missing original art inputs. Restore assets/original from the project source; see assets/README.md.' }
  # The asset pipeline encodes WebP with sharp (package.json).
  if (!(Test-Path './node_modules/sharp')) { npm ci; if ($LASTEXITCODE -ne 0) { throw 'npm ci failed' } }
  node scripts/prepare-game-assets.mjs; if ($LASTEXITCODE -ne 0) { throw 'Asset preparation failed' }
 }
 $assetManifest=Get-Content './assets/processed/game-assets.json' -Raw | ConvertFrom-Json
 $textures=@($assetManifest.animations.PSObject.Properties.Value.texture)+@($assetManifest.effect.texture,$assetManifest.background.texture,$assetManifest.shot.texture,$assetManifest.shadow.texture,$assetManifest.portrait.texture,'processed/name_font.otf')
 $missingAssets=@($textures | Where-Object { !(Test-Path -LiteralPath (Join-Path 'assets' $_) -PathType Leaf) })
 if ($missingAssets.Count) { throw "Missing runtime game assets ($($missingAssets.Count)). Restore or rebuild the included art pack; see assets/README.md." }
 $shortcuts=@(@{On=$Ai;Mode='cpu'},@{On=$Local;Mode='local'},@{On=$Demo;Mode='demo'}) | Where-Object { $_.On }
 if (@($shortcuts).Count -gt 1 -or (@($shortcuts).Count -eq 1 -and $PSBoundParameters.ContainsKey('Mode'))) { throw 'Choose one of -Mode, -Ai, -Local or -Demo' }
 if (@($shortcuts).Count -eq 1) { $Mode=$shortcuts[0].Mode }
 $online=if ($Server) { @('--server', $Server) } else { @() }
 $language=if ($Lang) { @('--lang', $Lang) } else { @() }
 cargo run --locked --bin naifrog-duel -- --mode $Mode @online @language
 if ($LASTEXITCODE -ne 0) { throw 'Game exited with an error' }
} finally { Pop-Location }
