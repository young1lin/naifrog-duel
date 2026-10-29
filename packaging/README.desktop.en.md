# Naifrog Duel {{VERSION}} · Windows client

[中文](README.md)

Extract the entire archive and run naifrog-duel.exe. Keep the assets directory beside it: it contains the original 2D animation, background and fonts. No Rust or Node.js installation is required.

- CPU mode: both keyboard layouts and any connected gamepad control your fighter.
- Local two-player: P1 uses A/D/W/S + F/G/H/T or controller 1; P2 uses arrows + J/K/L/I or controller 2.
- Gamepad: stick/D-pad move, A jump, X light, Y heavy, B dash, RB bubble, LB/LT/RT guard. Start restarts; Select pauses/resumes.
- Private online: pass a server URL and enter its access key in the menu. Desktop and browser clients share rooms.

    .\naifrog-duel.exe --server http://server.example.com:18080
    .\naifrog-duel.exe --lang en
    .\naifrog-duel.exe --version

Desktop networking currently supports HTTP only; use the browser for HTTPS servers. Public HTTP does not encrypt the access key or match traffic. Browser gamepad secure-context restrictions do not apply to the native EXE.

release-manifest.json records the version, source commit and file hashes. The adjacent .sha256 file verifies the archive. Project and third-party licenses are included.
