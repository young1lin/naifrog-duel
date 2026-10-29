#!/bin/sh
set -eu
cd -- "$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)"
: "${NAIFROG_ACCESS_KEY:?Set a private random key of at least 32 bytes before starting}"
export NAIFROG_ASSET_ROOT="${NAIFROG_ASSET_ROOT:-$PWD}"
export NAIFROG_BIND="${NAIFROG_BIND:-127.0.0.1:18080}"
export NAIFROG_ORIGIN="${NAIFROG_ORIGIN:-http://127.0.0.1:18080}"
exec ./naifrog-server
