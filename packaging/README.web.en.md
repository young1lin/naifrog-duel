# Naifrog Duel {{VERSION}} · Browser client

[中文](README.md)

This archive includes compiled WASM, web files and the original 2D runtime artwork. No Rust build is needed.

For offline play, install Node.js 22+ and run inside the extracted directory; npm install is unnecessary:

    node scripts/serve-web.mjs --host 127.0.0.1 --port 18080

Open http://127.0.0.1:18080. Serve the files over HTTP; opening the HTML directly does not work. This static server supports CPU/local play only.

For online play, download the matching server bundle, which already includes this web client. Keyboard and touch controls are available; browser gamepad availability normally requires HTTPS or localhost.

release-manifest.json and the adjacent .sha256 record the version, commit and hashes. Project/font licenses and artwork provenance are included.
