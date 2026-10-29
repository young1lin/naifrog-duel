# Naifrog Duel · Rust / Bevy 2D fighting game

[中文说明](README.md) · [Downloads](https://github.com/young1lin/naifrog-duel/releases) · [CI builds](https://github.com/young1lin/naifrog-duel/actions/workflows/ci.yml) · [Changelog](CHANGELOG.md)

Play against the CPU, share a screen with a second player, or join a private match on an authoritative Rust server. The native Windows EXE supports gamepads. Browser clients support keyboard, multi-touch and gamepads where the browser allows them. All versions retain the original 2D animation.

![Desktop game screenshot](docs/screenshots/game.png)

## Choose a download

| Component | Archive | Includes / use |
| --- | --- | --- |
| Windows desktop client | naifrog-desktop-vVERSION-windows-x86_64.zip | EXE, keyboard/gamepad input, CPU/local play, desktop online client, artwork |
| Browser client | naifrog-web-vVERSION.zip | Compiled WASM, web files, artwork and a Node offline server |
| Linux server | naifrog-server-vVERSION-linux-x86_64.tar.gz | x86_64 musl server plus the matching browser client and artwork |
| Windows server | naifrog-server-vVERSION-windows-x86_64.zip | Dedicated server EXE plus the matching browser client and artwork |

Use Releases for published versions. Before a release is published, open Actions → CI → Artifacts. Development names include -dev.<commit> and expire after 7 days. Access to this private repository is required while it remains private.

Extract the whole archive. Do not copy only the EXE or open web/index.html directly. Packages include Chinese/English instructions, licenses, a version/file manifest and an adjacent SHA256 checksum.

## Windows desktop

Run naifrog-duel.exe from the extracted desktop package. Rust and Node.js are unnecessary for this download.

- CPU: both keyboard layouts and any gamepad control P1.
- Local two-player: keyboard layout/controller 1 controls P1; layout/controller 2 controls P2.
- Private online: launch with the server URL, then enter its access key and create/join a room.

    .\naifrog-duel.exe --server http://server.example.com:18080
    .\naifrog-duel.exe --lang en
    .\naifrog-duel.exe --version

Desktop networking currently supports HTTP only; use a browser for an HTTPS server. HTTP does not encrypt access keys or match traffic.

### Controls

| Action | P1 / CPU mode | P2 |
| --- | --- | --- |
| Move | A / D | Left / Right |
| Jump | W | Up |
| Hold guard | S | Down |
| Light / heavy | F / G | J / K |
| Dash / bubble | H / T | L / I |
| Aerial belly attack | Light or heavy while airborne | Same |

Gamepad: stick/D-pad moves, A jumps, X light, Y heavy, B dash, RB bubble, LB/LT/RT guard. Start restarts; Select pauses/resumes. Esc opens the menu; R restarts, F1 shows hitboxes, F2 switches Chinese/English. Controller button labels vary by brand.

Menus accept keyboard, pointer/touch and controller navigation. Local matches pause on lost focus and require manual resume. Online matches remain authoritative on the server.

## Browser client

For an extracted browser package, Node.js 22+ can serve the files without npm install:

    node scripts/serve-web.mjs --host 127.0.0.1 --port 18080

Open http://127.0.0.1:18080. This is an offline CPU/local server; online play needs naifrog-server. Production server bundles already include the browser client.

A recent WebGL2 browser is required. Touch controls use a floating stick and four action buttons. Browser gamepads normally require HTTPS or localhost; this restriction does not apply to the native Windows client.

## Authoritative multiplayer server

Extract the server package and follow its README to set a private access key of at least 32 bytes. The included launchers default to local-only http://127.0.0.1:18080. LAN/public deployment also requires the correct bind address, Origin and proxy settings; see [deployment](DEPLOY.md).

The server supports Windows and browser clients in the same room. Clients submit controls, never health, positions or winners. The simulation runs at 60 Hz and broadcasts snapshots at 20 Hz. Rendering interpolates snapshots; there is no client prediction or rollback, so network latency affects input response.

One server instance holds all rooms in memory. Disconnections pause a match and retain the seat for 5 minutes. HTTPS browsers use secure session cookies. Access keys and resume credentials are not stored in browser localStorage or written to logs.

The separate Node static server has no authentication or online multiplayer and is for local/offline debugging.

## Gameplay and current limits

- Integer-based combat, first to two wins with a five-round cap, a 99-second timer, simultaneous hits and double-KO draws.
- Guard chip damage, knockdown/getup invulnerability, hit-stop, combos, jumps and one aerial attack per jump.
- CPU, local two-player and private online modes; Chinese/English interface and supported Chinese player names.
- Twelve animation atlases preserve the 2D artwork. Bubble and aerial-attack character frames currently reuse light/dash artwork as explicitly marked placeholders.
- Real controller models, cross-device play and public-network latency still need manual acceptance; CI cannot replace those checks.

The detailed frame data and input priority are in the [Chinese rules](README.md#已实现与规则); implementation and regression tests are in src/combat.rs.

## Build from source

Rust 1.98.1 is pinned in rust-toolchain.toml. Use Node.js 22+ for artwork/packaging tools. Windows builds require MSVC C++ build tools. Cargo defaults to two parallel jobs.

    npm ci
    .\scripts\run-game.ps1 -Mode cpu
    .\scripts\build-web.ps1 -Release

For a standalone server:

    .\scripts\cargo-private.ps1 -CargoArgs @('build','--locked','--release','--no-default-features','--features','server','--bin','naifrog-server')

The build wrapper remaps machine-specific compiler paths. The original images and processed runtime textures are included; no 3D conversion is involved.

## CI, versions and releases

CI performs actual Windows EXE, browser/WASM and Linux/Windows server builds, packages the runtime files, verifies privacy/checksums and uploads four downloadable artifacts. It also runs simulation/network tests, formatting, clippy, artwork validation and package/version regression tests.

Cargo.toml controls the application version. Cargo.lock, CHANGELOG.md and a release tag such as v0.1.0 must agree. The executables expose --version; PROTOCOL_VERSION is a separate wire-compatibility number.

    node scripts/version.mjs bump patch
    node scripts/version.mjs check

Complete the changelog, refresh the publication manifest, commit and push the matching tag. The Release workflow creates a draft with four archives and four checksum files. See the [release guide](docs/RELEASING.en.md) for the full sequence and version policy.

## License and provenance

Project code and maintainer-provided AI artwork are offered under [MIT](LICENSE) to the extent the maintainer can license the applicable rights. The maintainer reports ChatGPT-generated originals and assistant-generated atlases; [ART_PROVENANCE.json](assets/ART_PROVENANCE.json) records that statement and hashes. Fonts retain their licenses. See [NOTICE](NOTICE.md), [asset policy](assets/README.md), [contributing](CONTRIBUTING.md), [privacy/publication boundary](docs/OPEN_SOURCE.md) and [security reporting](SECURITY.md).
