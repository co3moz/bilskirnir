# Bilskirnir

Ubuntu container desktops with GPU access, browser VNC, layered snapshots and persistent volumes. Built with Rust and Docker. The name is a nod to Thor.

## Setup

Requires x86_64 WSL2 Ubuntu, Docker Engine and Compose v2. WSL graphics require `/dev/dxg` and `/usr/lib/wsl`. CUDA additionally requires NVIDIA Container Toolkit on the host Engine.

Packages are currently private. The installing GitHub account must have package access. Sign in to GHCR using a classic personal access token with `read:packages` as the password:

```bash
docker login ghcr.io -u YOUR_GITHUB_USERNAME
```

This step is unnecessary once the packages are public. Install without building or accessing the source checkout. Run in a new directory in WSL:

```bash
mkdir bilskirnir && cd bilskirnir
docker pull ghcr.io/co3moz/bilskirnir:latest
docker run --rm --entrypoint cat ghcr.io/co3moz/bilskirnir:latest /app/install/compose.yml > compose.yml
docker run --rm --entrypoint cat ghcr.io/co3moz/bilskirnir:latest /app/install/.env.example > .env
# Set ADMIN_PASSWORD, or use AUTH_MODE=none.
nano .env
docker compose --profile images pull manager desktop desktop-gnome
docker compose up -d --no-build manager
```

Open <http://localhost:8080>. Credentials come from `ADMIN_USERNAME` and `ADMIN_PASSWORD`. Sessions expire after 12 hours. Signing out or session expiry closes active VNC, audio and microphone connections. Recreate the manager after changing its environment variables.

Set `BIND_ADDRESS=0.0.0.0` for LAN access. Add DNS names to `ALLOWED_HOSTS`; localhost and private IP addresses are accepted by default. Windows firewall and WSL forwarding remain host configuration.

From a source checkout, use `docker compose -f compose.ghcr.yml --profile images pull manager desktop desktop-gnome` and `docker compose -f compose.ghcr.yml up -d manager` for the same prebuilt installation. Desktop profile services are used to pull images; the manager creates the actual desktop containers.

To update a prebuilt installation, run the same pull command and recreate the manager with `docker compose up -d --no-build manager`. Existing desktops and snapshots retain their image IDs and data; newly created desktops use the pulled profile images. Do not delete the manager data volume when upgrading. `MANAGER_IMAGE` can pin a version tag or image digest instead of `latest`.

## Build from source

From the checkout in WSL:

```bash
cp .env.example .env
# Set ADMIN_PASSWORD, or use AUTH_MODE=none.
docker compose --profile images build desktop desktop-gnome manager
docker compose up -d --no-build manager
```

Source builds use the same desktop image references as the registry installation. The source manager is tagged `bilskirnir/manager:local`.

## Container publishing

GitHub Actions runs frontend/backend checks and publishes three `linux/amd64` images using the repository's `GITHUB_TOKEN`:

| Image | Moving tag on `master` |
| --- | --- |
| Manager | `ghcr.io/co3moz/bilskirnir:latest` |
| Lightweight XFCE | `ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04` |
| GNOME | `ghcr.io/co3moz/bilskirnir/ubuntu-gnome:24.04` |

Every publication also has a `sha-<full-commit>` tag. Desktop images receive `latest` in addition to `24.04`. A tag such as `v1.2.3` publishes `1.2.3` and `1.2` for all three images, without replacing the moving `master` tags. **Publish containers** can also be dispatched manually on `master`. Image-affecting changes on `master` trigger publication; documentation-only changes do not rebuild images. Each image has a separate BuildKit cache; official actions are pinned to verified commits. After publishing, the workflow reads each registry manifest back by digest to verify authenticated access.

GHCR creates new packages as private. For unauthenticated installation, set **Package settings → Change visibility → Public** for the manager and both desktop packages after their first publication. The source repository can remain private. See [GitHub's container registry documentation](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry). If a package remains private, the installing host needs `docker login ghcr.io` with a classic personal access token that has `read:packages`.

Hosted CI checks builds and unit tests. WSL GPU acceptance remains a separate host test.

## Desktop profiles

| Profile | Image |
| --- | --- |
| Ubuntu 24.04 · Lightweight XFCE | `ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04` |
| Ubuntu 24.04 · GNOME | `ghcr.io/co3moz/bilskirnir/ubuntu-gnome:24.04` |

Pull both profiles with the setup command, or pull only `desktop manager` for XFCE and prepare GNOME later by pulling its image or building it from **Images**. `DESKTOP_IMAGE` controls the default selection. Each profile has its own image and preserves its desktop metadata through custom builds and snapshots.

GNOME uses an independent X11 session with the Ubuntu shell and Yaru theme. GDM, systemd/logind, Wayland sessions and host power/USB management are outside the container desktop. Screen locking is disabled for this session; panel authentication controls access. GNOME requires more resources than XFCE.

To add a profile, extend `desktop/profiles.json` with its ID, name, reference, `session`, `build_target` and `build_service`. Add the corresponding Dockerfile stage/session and desktop metadata labels, then rebuild the manager.

## Environments

Choose an image or snapshot, resolution, GPU, internet, CPU/memory limits, sudo, audio/microphone and named volumes. Use **Connect** to open the desktop. The actions menu provides start, stop, restart, snapshots, logs, diagnostics and deletion.

Environment count has no application quota. Empty CPU/memory fields use Docker's unlimited defaults; available host resources still constrain workloads. GPU access does not reserve or limit VRAM.

A private internal bridge connects each desktop to the manager; VNC is not published on the host. A separate egress bridge is attached or detached by the internet switch. This controls Docker egress, not a separate LAN firewall. Desktop containers do not receive the Docker socket. The manager uses the host Engine socket rather than a nested daemon.

## GPU

TigerVNC/Xvnc provides an independent display without a physical monitor. VirtualGL EGL and Mesa D3D12 provide hardware OpenGL. GPU-enabled startup rejects software rendering; use **Diagnostics** for details.

```bash
gpu-run glxinfo -B
gpu-run /path/to/Unity -force-glcore
gpu-run /path/to/MyLinuxGame -force-glcore
```

`GPU_ADAPTER=NVIDIA` selects a Mesa adapter by name. `NVIDIA_COMPUTE=false` disables the CUDA runtime request while retaining WSL graphics when `WSL_GPU=true`.

Unity Editor/Player versions and projects need application-level validation. Vulkan, other GPU vendors and native Linux graphics have not been verified. Windows executables/Wine are not included.

## Images and snapshots

**Build image** runs a shell script as root on a selected desktop base. Build internet access is optional. Jobs show a live log and the resulting image. Keep the desktop infrastructure intact when installing applications. **Import image** registers a compatible image already available in the Engine.

**Snapshot** uses Docker commit. It briefly pauses the container and creates a new layer on its immutable source image. Volumes, `/run`, `/tmp` and running application/RAM/GPU state are excluded. Creating an environment from a snapshot makes a branch; taking two snapshots of the same original container creates sibling branches.

Home is stored in the container layer by default. An image-declared `VOLUME` must have an explicit named-volume mount, preventing anonymous volume data from silently disappearing from snapshots. Images removed from the Engine are unavailable for new environments.

## Volumes

Create volumes under **Volumes**, then select mounts in the new environment form. Use canonical absolute paths such as `/workspace/project` or `/opt/game`. System paths and read-only desktop home/configuration mounts are rejected.

The desktop user has UID/GID **1001:1001**. Existing volumes need suitable permissions. Sharing a writable volume between environments shares its data; snapshots do not clone it. Deleting an environment preserves named volumes and saved images.

Transfer host files using Docker's tools:

```bash
docker cp ./project wsld-ENVIRONMENT_ID:/workspace/project
docker cp wsld-ENVIRONMENT_ID:/home/desktop/Downloads/result ./result
```

## Audio and microphone

`AUDIO_ENABLED` and `MICROPHONE_ENABLED` control installation support; each environment opts in separately. Disabling audio also disables microphone support. Browser buttons enable audio/microphone independently. Leaving the desktop closes both channels and microphone tracks.

Audio uses 48 kHz float32 PCM over authenticated WebSockets: stereo output (~384 KB/s), mono input (~192 KB/s). Browser microphone permission is required. Localhost works over HTTP; other devices require HTTPS.

For LAN HTTPS with Caddy's local CA:

For the standalone registry installation, export the bundled Caddy configuration first:

```bash
mkdir -p deploy
docker run --rm --entrypoint cat ghcr.io/co3moz/bilskirnir:latest /app/install/deploy/Caddyfile > deploy/Caddyfile
```

```dotenv
BIND_ADDRESS=0.0.0.0
DESKTOP_HOST=desktop.lan
HTTPS_PORT=8443
SECURE_COOKIE=true
```

```bash
docker compose --profile https up -d manager https
docker compose cp https:/data/caddy/pki/authorities/local/root.crt ./caddy-root.crt
```

Point the hostname at the host and trust the CA on the client device. Open <https://desktop.lan:8443>. Secure cookies require HTTPS.

## Recovery

SQLite metadata is stored in `/data/state.sqlite`. A manager restart marks unfinished jobs as interrupted; scripts are not replayed automatically. The manager reconnects to desktop private networks every 20 seconds. One invalid environment does not block the others.

**Retry setup** preserves an existing container layer, completes missing networks/connections and retries startup. A missing container is recreated from its saved immutable image ID. Missing images or volumes stop recovery rather than replacing data silently. A deleted, unsnapshotted container layer cannot be recovered.

Operations on one environment are serialized; independent environments and image builds can progress concurrently. Standard CLI operations have a 120-second timeout; build/commit has a one-hour timeout. Job logs retain the last 256 KiB. Ownership and network endpoints are checked before deletion. No Engine-wide prune is performed.

## Upgrade compatibility

Existing database records, snapshots and desktop containers remain compatible. Persisted ownership labels (`io.wsl-desktop.*`), `wsld-*` resource IDs, the session cookie and API header are intentionally retained as compatibility identifiers.

New packages, binaries, Compose defaults and image tags use Bilskirnir. Existing profile images can be retagged without rebuilding:

```bash
docker image tag bilskirnir/ubuntu-xfce:24.04 ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04
docker image tag bilskirnir/ubuntu-gnome:24.04 ghcr.io/co3moz/bilskirnir/ubuntu-gnome:24.04
```

For older `wsl-desktop/ubuntu-*` tags, use the same retagging commands with the old tag as the source. Before replacing a previous Compose installation, set `MANAGER_DATA_VOLUME` to its existing manager data volume name. The new default is `bilskirnir-data`. Remove the previous manager/Compose deployment without deleting its data volume, then start Bilskirnir against that volume. Existing desktops can continue running during the manager replacement.

The development preview is at <http://localhost:8088>, bound only to localhost with login disabled. Its manager is `bilskirnir-preview`; the existing `wsl-desktop-preview-data` volume is reused. Environment names and settings remain user-controlled.

## Interface design

The [design system](docs/DESIGN.md) defines the palette, typography, components and responsive behavior used by the frontend.

## Development

Read [AGENTS.md](AGENTS.md) for the project map, implementation constraints, verification and commit rules.

```bash
npm ci
npm run build
npm test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Build both desktop profiles and the manager before running the real Engine acceptance suite in WSL:

```bash
python3 tests/integration.py
```

The suite creates uniquely named resources and cleans only its own resources after ownership checks. It covers both GPU desktops, authentication, VNC/JPEG/audio, snapshots, volumes, network toggling, concurrent operations, recovery and bounded build logs. Interrupted acceptance runs can be cleaned with `tests/cleanup_acceptance.py` and the exact test manager name.

Verified on WSL Ubuntu 22.04, Docker Engine 28.1.1 and NVIDIA RTX 4080: XFCE/GNOME desktops and OpenGL 4.6 via VirtualGL/Mesa D3D12. Physical browser microphone hardware and Unity itself have not been tested.
