# Naifrog Duel {{VERSION}} · Authoritative server

[中文](README.md)

Includes the server executable, the browser client built from the same commit, and the original 2D runtime artwork. No separate web download is needed. Linux uses x86_64 musl; Windows uses x86_64 MSVC.

The launchers bind to 127.0.0.1:18080 by default. Set a private access key of at least 32 bytes first. No fixed key is generated or saved.

Windows PowerShell:

    $key = Read-Host 'Service key (at least 32 bytes)' -AsSecureString
    $env:NAIFROG_ACCESS_KEY = [Net.NetworkCredential]::new('', $key).Password
    .\start-server.ps1

Linux Bash:

    read -rs -p 'Service key (at least 32 bytes): ' NAIFROG_ACCESS_KEY
    export NAIFROG_ACCESS_KEY
    ./start-server.sh

Open http://127.0.0.1:18080 and enter your key. LAN/public hosting also requires NAIFROG_BIND, NAIFROG_ORIGIN and proxy configuration; see DEPLOY.md. Run exactly one server instance.

The Windows client connects over HTTP; use a browser for HTTPS. HTTP does not encrypt access keys or game traffic. Keep credentials out of URLs, screenshots and public logs.

The executable supports --version. release-manifest.json and the adjacent .sha256 verify version/integrity. Project and third-party licenses are included.
