#!/usr/bin/env bash
# Build the Bilskirnir WSL distribution package from wsl/Dockerfile.
# Usage: scripts/build-wsl.sh [output.wsl]   (set WSL_COMPRESS=xz for a smaller, slower build)
set -euo pipefail
cd "$(dirname "$0")/.."
output=${1:-dist/bilskirnir.wsl}
image=bilskirnir/wsl-rootfs:local
docker build --pull -t "$image" -f wsl/Dockerfile .
container=$(docker create "$image")
trap 'docker rm -f "$container" >/dev/null' EXIT
mkdir -p "$(dirname "$output")"
case ${WSL_COMPRESS:-gzip} in
    xz) docker export "$container" | xz -T0 -6 > "$output" ;;
    *) docker export "$container" | gzip -6 > "$output" ;;
esac
ls -lh "$output"
