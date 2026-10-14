#!/usr/bin/env bash
# Build the Bilskirnir WSL distribution package from wsl/Dockerfile.
# Usage: scripts/build-wsl.sh [output.wsl]   (set WSL_COMPRESS=gzip for a faster, larger build)
set -euo pipefail
cd "$(dirname "$0")/.."
output=${1:-dist/bilskirnir.wsl}
image=bilskirnir/wsl-rootfs:local
# WSL has read xz root filesystems since 0.61.4, well before the 2.4.4 that .wsl files need. The
# 64 MiB dictionary of -9e finds the code the Docker and NVIDIA Go binaries share, which gzip cannot.
case ${WSL_COMPRESS:-xz} in
    xz) compress=(xz -T0 -9e) ;;
    gzip) compress=(gzip -9) ;;
    *) echo "WSL_COMPRESS must be xz or gzip" >&2; exit 2 ;;
esac
docker build --pull -t "$image" -f wsl/Dockerfile .
container=$(docker create "$image")
trap 'docker rm -f "$container" >/dev/null' EXIT
mkdir -p "$(dirname "$output")"
docker export "$container" | "${compress[@]}" > "$output"
ls -lh "$output"
