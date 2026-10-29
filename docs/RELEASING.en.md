# Versioning and releases

[中文](RELEASING.md)

## Three roles, four downloads

- Windows desktop: the default game feature keeps keyboard/gamepad input, CPU/local play and desktop networking. The EXE ships with the original 2D runtime artwork.
- Browser: compile real WASM, run the matching wasm-bindgen, then package web files and assets. The static debug server supports offline play only.
- Authoritative server: build Linux x86_64 musl and Windows x86_64 MSVC without Bevy. Each bundle includes the browser client from the same commit.

Each archive has a SHA256 file and an internal release-manifest.json with the application version, commit, target and file hashes. Server packaging rejects web archives from a different version/commit. Both README languages and required licenses are included.

## CI artifacts and Releases

Branch pushes, PRs and manual CI run checks followed by actual release compilation. Download development archives from Actions → CI → Artifacts. Names include -dev.<commit>; --version reports the Cargo base version, and the manifest records the exact commit. CI artifacts expire after 7 days.

Build packages also supports manual all/web/desktop/server selection. Server builds include the matching web build. Compilation caches are saved before packaging, so a packaging failure does not discard a successful compilation.

A v-prefixed version tag triggers the Release workflow. It repeats checks/builds and creates a draft Release only after all four packages succeed. It never changes repository visibility or automatically publishes the draft.

Packaged web/server smoke checks exercise real WASM delivery, pages and server authentication. The Windows EXE runs --version to verify linking and version metadata. CI cannot validate a physical controller or a real cross-device match.

## Version policy

Cargo.toml package.version is authoritative; the project's Cargo.lock entry must match and CHANGELOG.md must contain its release notes. The initial tag is v0.1.0; after its release check found a problem, v0.1.1 fixes it without rewriting that tag.

- patch: compatible fixes, for example 0.1.0 → 0.1.1.
- minor: features; before 1.0, also use this for incompatible changes.
- major: incompatible stable-version changes, for example 1.x → 2.0.0.
- PROTOCOL_VERSION is separate. Update it when changing wire compatibility and release matching clients/servers.

In the clean repository copy:

    node scripts/version.mjs bump patch

This updates Cargo.toml/Cargo.lock and moves Unreleased notes under the new version. Complete the notes, then in PowerShell:

    node scripts/version.mjs check
    node --test scripts/*.test.mjs
    node scripts/public-source.mjs manifest
    git add .
    git commit -m "chore: prepare next release"
    $version = node scripts/version.mjs print
    git tag "v$version"
    git push origin main "v$version"

Incomplete notes, a mismatched lock version, or a tag that differs from Cargo block the release. Keep published tags immutable; fix a released version with a new version.

## Draft acceptance

1. Verify the archive SHA256, packaged version and source commit.
2. Extract and run Windows desktop; test keyboard, a real controller and the required modes.
3. Run the server and test browser/desktop room sharing, reconnects and the original 2D animation.
4. Review the notes and manually publish the draft.

Only drafts can be updated by rerunning the workflow; published Releases are never overwritten. Full builds run on GitHub runners. Local Cargo defaults to two parallel jobs.
