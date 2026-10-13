# Installation and maintenance

[Back to the README](../README.md)

The [WSL distribution package](WSL_PACKAGE.md) installs Ubuntu, Docker and the GPU runtime together; the [README's Compose example](../README.md#getting-started) is the shortest route inside an existing WSL Ubuntu. This guide covers changes you may need after that: settings, access from another device, updates and recovery.

## Use the full installation files

If you have a source checkout, copy `.env.example` to `.env`, set `ADMIN_PASSWORD` and start the published images:

```bash
cp .env.example .env
docker compose -f compose.ghcr.yml up -d
```

For an installation without a checkout, the manager image includes the same files. Export them into your installation directory:

```bash
mkdir -p deploy
docker run --rm --entrypoint cat ghcr.io/co3moz/bilskirnir:latest /app/install/compose.yml > compose.ghcr.yml
docker run --rm --entrypoint cat ghcr.io/co3moz/bilskirnir:latest /app/install/.env.example > .env
docker run --rm --entrypoint cat ghcr.io/co3moz/bilskirnir:latest /app/install/deploy/Caddyfile > deploy/Caddyfile
```

Edit the exported `.env` before starting the manager. Run these export commands in a new directory so you do not overwrite settings from an existing installation.

## Settings

The minimal README example keeps settings directly in Compose. The full manifests read them from `.env`. Recreate the manager with `docker compose up -d` after changing its environment; for the full registry manifest, use `docker compose -f compose.ghcr.yml up -d`.

| Setting | Default | What it changes |
| --- | --- | --- |
| `AUTH_MODE` | `password` | Use `none` to open the manager without a sign-in screen. |
| `ADMIN_USERNAME` | `admin` | The single administrator's sign-in name. |
| `ADMIN_PASSWORD` | Empty | Set a password of at least eight characters when login is enabled. |
| `MCP_ENABLED` | `false` | Enable the embedded agent endpoint at `/mcp`. |
| `MCP_TOKEN` | Empty | Separate bearer token, required when MCP is enabled; 32–256 visible ASCII characters. |
| `WSL_GPU` | `true` | Makes WSL graphics access available to GPU-enabled desktops. When this and `NVIDIA_COMPUTE` are both off, the GPU option is unavailable. |
| `NVIDIA_COMPUTE` | `true` | Requests CUDA through NVIDIA Container Toolkit; turn it off without that runtime. |
| `GPU_ADAPTER` | Empty | Selects a Mesa adapter by name; empty leaves selection to Mesa. |
| `AUDIO_ENABLED` | `true` | Allows environments to use browser sound. |
| `MICROPHONE_ENABLED` | `true` | Allows microphone input when audio is also enabled. |
| `UPLOAD_MAX_MB` | `1024` | Maximum file size in MiB, from 1 to 65536. |
| `DESKTOP_IMAGE` | XFCE GHCR image | The default image when creating an environment. |
| `ALLOWED_HOSTS` | `localhost` in Compose | Extra DNS names accepted by the manager, separated by commas. |
| `SECURE_COOKIE` | `false` | Require HTTPS for the sign-in cookie. |

The full manifests also accept these Compose settings:

| Setting | Default | What it changes |
| --- | --- | --- |
| `BIND_ADDRESS` | `127.0.0.1` | Which host interfaces expose the manager and optional HTTPS service. |
| `PORT` | `8080` | The manager's HTTP port on the host. |
| `MANAGER_CONTAINER` | `bilskirnir-manager` | The manager container's name. |
| `MANAGER_DATA_VOLUME` | `bilskirnir-data` | The volume holding the manager database and temporary uploads. |
| `MANAGER_IMAGE` | `ghcr.io/co3moz/bilskirnir:latest` | The manager image in `compose.ghcr.yml`; accepts a version or digest. |
| `DESKTOP_HOST` | `localhost` | The hostname for the optional HTTPS service; also added to accepted hosts. |
| `HTTPS_PORT` | `8443` | The optional HTTPS service's port on the host. |

Sessions last 12 hours. Signing out or reaching session expiry closes active desktop and media connections. Desktop containers continue running.

Environments created with the gamepad option receive the host's `/dev/uinput` device, which current WSL2 kernels provide; a root helper inside the desktop creates the virtual controller there.

Agent access uses a separate token rather than the dashboard session. See the [MCP guide](MCP.md) to connect an agent, or the [HTTP API](API.md) to build another client.

## Access from another device

For the minimal Compose example, change the port binding to `0.0.0.0:8080:8080`. For the full manifests, set `BIND_ADDRESS=0.0.0.0`. Open the manager using the host's LAN address.

The manager accepts localhost and private LAN IP addresses. If you use a DNS name, add it to `ALLOWED_HOSTS`. Windows firewall rules and forwarding into WSL must also allow the connection; Compose does not configure those host settings.

Keep login enabled when other people can reach the manager. Its Docker socket connection gives it control of the host Engine. The current application has one administrator, with no separate accounts or per-user environment permissions.

## HTTPS and microphone access over LAN

Use the full manifest for the optional Caddy HTTPS service. It creates a local certificate authority, which each client device needs to trust. This also lets a remote browser request microphone access.

Set these values in `.env`, using a hostname that points to your Windows/WSL host:

```dotenv
BIND_ADDRESS=0.0.0.0
DESKTOP_HOST=desktop.lan
HTTPS_PORT=8443
SECURE_COOKIE=true
```

Start the manager and Caddy, then copy the CA certificate:

```bash
docker compose -f compose.ghcr.yml --profile https up -d manager https
docker compose -f compose.ghcr.yml cp https:/data/caddy/pki/authorities/local/root.crt ./caddy-root.crt
```

Trust `caddy-root.crt` on the client device and open [desktop.lan:8443](https://desktop.lan:8443). Once secure cookies are enabled, sign in through HTTPS.

Source installations use the same HTTPS profile in `compose.yml`. Omit `-f compose.ghcr.yml` for that manifest.

## Update the images

For the minimal Compose installation:

```bash
docker compose pull
docker compose up -d
```

For the full registry installation, add `-f compose.ghcr.yml` to both commands.

This updates the manager. Desktop profile images are not Compose services: the manager downloads a profile the first time a desktop is created from it. To get a newer release of a profile you already have, select **Update** on its card under **Images**, or run `docker pull` with the profile's image reference. In the [WSL package](WSL_PACKAGE.md), `bilskirnir update` does both.

Compose files from earlier releases list two short-lived `desktop` and `desktop-gnome` services that download both profiles at every start. They keep working. To download profiles only when they are used, replace the file with the current `compose.ghcr.yml` (or remove those services and the manager's `depends_on`), then run `docker compose up -d --remove-orphans` to remove their exited containers. Images already downloaded stay in place.

New environments use the updated profile images. Existing desktops and snapshots keep their saved image IDs; updating the manager does not reinstall their applications or replace their files. Keep the manager data volume when updating. To stay on a particular build, pin image references to a published version, `sha-<full-commit>` tag or digest.

## Know where your data lives

| Data | Location | Effect of deleting an environment |
| --- | --- | --- |
| Environment settings, image records and jobs | `/data/state.sqlite` in the manager data volume | The manager's database remains. |
| Installed software and unmounted home files | The desktop container's writable layer | Lost unless saved in an image or snapshot. |
| Files carried over by a settings change | An image tagged `bilskirnir/environment:JOB_ID` that the container runs | Removed, unless a snapshot builds on it. |
| Files in named-volume mounts | Separate Docker volumes | Preserved. |
| Saved image and snapshot layers | The host Docker Engine's image store | Preserved. |

A snapshot alone is not a backup of the whole installation. It excludes volumes and the manager database. Protect the manager data volume, your project volumes and any images you need to recreate desktops.

Uploads are temporarily staged in the manager data volume. The manager accepts two transfers at once, each with a one-hour limit. Manager-side temporary upload files are removed after completion or cancellation, and leftover staging files are cleaned on startup. Make room for staging as well as the destination files.

## When setup is interrupted

A manager restart marks unfinished jobs as interrupted. Build scripts are not replayed automatically. Inspect the job result before starting an operation again.

Use **Retry setup** for an environment that failed during creation or startup. If its container still exists, Bilskirnir keeps the writable layer and completes missing networks or startup steps. If the container is missing, it can recreate it from the saved source image ID.

A settings change that needs a new container renames the old one to `wsld-ID-previous` until its replacement has started. If the manager stops during that job, the old container and its files are still there under that name. Rename it back with `docker rename wsld-ID-previous wsld-ID` before using **Retry setup**, or remove it once its files are no longer needed; another settings change refuses to start while it exists.

Recreating a missing container cannot restore changes that were never saved as a snapshot. Missing images or volumes stop recovery so you can resolve them; Bilskirnir does not silently replace a missing volume with an empty one.

The manager checks desktop network connections every 20 seconds. A problem with one environment does not prevent checks for the others. Changes made directly through Docker can still conflict with manager operations, so inspect affected resources before trying recovery.

## If something does not start

If the manager is not reachable, check its startup log:

```bash
docker compose logs --tail=100 manager
```

Use `-f compose.ghcr.yml` with the full registry manifest. A missing or too-short password prevents startup when `AUTH_MODE=password`. A registry error happens before startup; check that the host can reach GHCR.

If the dashboard opens but an environment fails, read its job log and **Logs**; **Diagnostics** needs a running desktop. For a GPU error, confirm the WSL device and libraries are available and that CUDA is configured if `NVIDIA_COMPUTE=true`. You can create a separate environment with GPU disabled to check the basic desktop path without deleting the failed environment's files.

If file uploads report a permission error, check that the chosen volume is writable by UID/GID 1001:1001. Raising the upload size limit will not fix destination permissions.

## Move an older installation

Before replacing an older Compose deployment, set `MANAGER_DATA_VOLUME` to its existing manager data volume name. The current default is `bilskirnir-data`. Remove the old manager deployment without deleting that volume, then start Bilskirnir using it. Existing desktops can keep running while the manager is replaced.

You may see `wsld-*` resource names or `io.wsl-desktop.*` Docker labels. These identifiers are retained so existing containers, snapshots and database records continue to work. They do not indicate a different product.

If locally built desktop images still use older tags, retag them rather than rebuilding:

```bash
docker image tag bilskirnir/ubuntu-xfce:24.04 ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04
docker image tag bilskirnir/ubuntu-gnome:24.04 ghcr.io/co3moz/bilskirnir/ubuntu-gnome:24.04
```

For images tagged `wsl-desktop/ubuntu-*`, substitute that existing source tag on the left. Saved environment image IDs do not change when you add a tag.
