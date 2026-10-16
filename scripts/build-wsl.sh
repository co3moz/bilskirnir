#!/usr/bin/env bash
# Build the Bilskirnir WSL distribution package from wsl/Dockerfile.
# Usage: scripts/build-wsl.sh [output.wsl]   (set WSL_COMPRESS=gzip for a faster, larger build)
# The package installs the manager release BILSKIRNIR_VERSION, by default the version in Cargo.toml.
set -euo pipefail
cd "$(dirname "$0")/.."
output=${1:-dist/bilskirnir.wsl}
image=bilskirnir/wsl-rootfs:local
version=${BILSKIRNIR_VERSION:-$(awk -F'"' '/^version = /{ print $2; exit }' Cargo.toml)}
# The version becomes an image tag: letters, digits, underscores, periods and dashes.
if ! [[ $version =~ ^[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}$ ]]; then
    echo "BILSKIRNIR_VERSION must be an image tag such as 1.2.3, not '$version'" >&2
    exit 2
fi
# WSL has read xz root filesystems since 0.61.4, well before the 2.4.4 that .wsl files need. The
# 64 MiB dictionary of -9e finds the code the Docker and NVIDIA Go binaries share, which gzip cannot.
case ${WSL_COMPRESS:-xz} in
    xz) compress=(xz -T0 -9e) ;;
    gzip) compress=(gzip -9) ;;
    *) echo "WSL_COMPRESS must be xz or gzip" >&2; exit 2 ;;
esac
docker build --pull --build-arg "BILSKIRNIR_VERSION=$version" -t "$image" -f wsl/Dockerfile .
container=$(docker create "$image")
trap 'docker rm -f "$container" >/dev/null' EXIT
# A COPY without --chmod takes its mode from the checkout, which is 0777 on a Windows drive, and one
# with --chmod=0644 gives that mode to a directory it creates. Nothing in the package may be writable
# by others except symlinks and sticky directories such as /tmp, and every directory must be enterable.
bad_modes=$(docker export "$container" | tar -tv | awk '
    (substr($1, 9, 1) == "w" && $1 !~ /^l/ && $1 !~ /[tT]$/) || ($1 ~ /^d/ && substr($1, 4, 1) !~ /[xs]/)')
if [[ -n $bad_modes ]]; then
    printf 'Wrong modes in the package:\n%s\n' "$bad_modes" >&2
    exit 1
fi
mkdir -p "$(dirname "$output")"
docker export "$container" | "${compress[@]}" > "$output"
ls -lh "$output"
echo "The package installs manager $version."
