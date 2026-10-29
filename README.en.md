# Naifrog Duel

A 2D fighting-game prototype built with Rust and Bevy. It supports a CPU opponent, local two-player play, and private online matches using an authoritative Rust server. Keyboard, gamepads and browser multi-touch share the same input model.

[中文说明](README.md) · [Asset policy](assets/README.md) · [Contributing](CONTRIBUTING.md) · [Deployment](DEPLOY.md)

## Project source and artwork

Project code and the maintainer-provided AI artwork are offered under [MIT](LICENSE), to the extent the maintainer can license the applicable rights. The maintainer reports that the originals were generated using ChatGPT and the original atlases were assistant-generated. The model label and source hashes are recorded in [ART_PROVENANCE.json](assets/ART_PROVENANCE.json). Third-party fonts retain their own licenses; see [NOTICE.md](NOTICE.md).

The source includes the game's original 2D animation textures and the source images needed to rebuild them.

![Desktop game screenshot](docs/screenshots/game.png)

Rust 1.98.1 is pinned in `rust-toolchain.toml`; Node.js 22+ is required for asset tooling. Windows desktop builds need the MSVC C++ build tools.

```powershell
npm ci
cargo test --locked --no-default-features --features "server client" --lib
cargo fmt --all --check
cargo clippy --locked --no-default-features --features "server client" --all-targets -- -D warnings
node --test scripts/server.test.mjs scripts/network.test.mjs
cargo check --locked --all-targets
```

These tests do not require character art. For a browser compilation check:

```powershell
rustup target add wasm32-unknown-unknown
cargo check --locked --target wasm32-unknown-unknown --bin naifrog-duel
```

`scripts/run-game.ps1` starts the desktop game using the included textures, and `scripts/build-web.ps1 -Release` builds the web version. `npm ci`, `node scripts/prepare-game-assets.mjs`, and `node scripts/check-assets.mjs` rebuild and validate the art pack. `scripts/serve-web.mjs` is an offline local debug server. Production multiplayer uses `naifrog-server`; deployment instructions and limitations are documented separately.

The simulation is integer-based at 60 Hz. The server receives controls and publishes authoritative snapshots. Generated packages, credentials, unused art drafts, local archives and agent skills are outside the public export.
