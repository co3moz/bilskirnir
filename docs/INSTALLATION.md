# Installation and maintenance

[Back to the README](../README.md)

The [WSL distribution package](WSL_PACKAGE.md) installs Ubuntu, Docker and the GPU runtime together; the [README's Compose example](../README.md#custom-installation) is the shortest route on a Linux host you manage yourself, in WSL or not. This guide covers changes you may need after that: settings, access from another device, updates and recovery.

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

The dashboard's **Settings** page lists the settings below with the values the manager runs with, so you can see what a restart picked up. It never shows `ADMIN_PASSWORD` or `MCP_TOKEN`, only whether they are set. Of the Compose settings in the second table it shows `MANAGER_CONTAINER` and `DESKTOP_HOST`, since Compose keeps the others to itself.

In the [WSL package](WSL_PACKAGE.md#settings-from-the-dashboard) the page also changes `SECURE_COOKIE`, `ALLOWED_HOSTS`, `DESKTOP_IMAGE`, `DESKTOP_USER_NAMESPACES`, `WSL_GPU`, `NVIDIA_COMPUTE`, `GPU_ADAPTER`, `AUDIO_ENABLED`, `MICROPHONE_ENABLED`, the other container features' switches and defaults below, `UPLOAD_MAX_MB`, `IMAGE_ARCHIVE_MAX_MB`, `TEMPLATES_URL`, `VOLUME_FILTER` and `UPDATE_CHECK`: it checks the new values as the manager would read them when it starts, writes them to `.env` and recreates the manager. `.env` stays the only place they are kept. The others, sign-in, the root MCP and its token, `MANAGER_CONTAINER` and the Compose settings, change only in `.env`, with `bilskirnir settings` in the package, and the page marks them **CLI only**. Elsewhere the page changes nothing.

| Setting | Default | What it changes |
| --- | --- | --- |
| `AUTH_MODE` | `password` | Use `none` to open the manager without a sign-in screen. |
| `ADMIN_USERNAME` | `admin` | The single administrator's sign-in name. |
| `ADMIN_PASSWORD` | Empty | Set a password of at least eight characters when login is enabled. |
| `ROOT_MCP_ENABLED` | `false` | Enable the root agent endpoint at `/mcp`, which reaches every container. The [MCP endpoints](MCP.md#mcp-endpoints-for-one-agent) made on the dashboard answer with their own tokens whether or not it is on. Called `MCP_ENABLED` before 0.6.3: an `.env` that still has the old name keeps its value while `ROOT_MCP_ENABLED` is missing, the manager logs that it should be renamed, and refuses to start when the two names disagree. In the WSL package `bilskirnir update` renames it. |
| `MCP_TOKEN` | Empty | The administrator's bearer token, required when `ROOT_MCP_ENABLED=true` in `token` mode; 1 to 256 visible ASCII characters without spaces. It opens every MCP endpoint too, also while `/mcp` is off. Wrap it in single quotes when it contains `$` or `#`. |
| `MCP_AUTH_MODE` | `token` | Use `none` to accept agent requests without a token, on `/mcp` and on every MCP endpoint, whether or not `/mcp` is on. Anyone who can reach the manager's address can then control every container that `/mcp` or an endpoint reaches, so keep it on localhost. |
| `WSL_GPU` | `true` | Gives GPU containers the WSL GPU (`/dev/dxg` and `/usr/lib/wsl`) where the host has it. See [GPU on a Linux host](#gpu-on-a-linux-host) for how the routes are chosen. |
| `NVIDIA_COMPUTE` | `true` | Gives GPU containers an NVIDIA GPU through the NVIDIA Container Toolkit: CUDA, and outside WSL OpenGL too. |
| `DRI_GPU` | `true` | Experimental. Gives GPU containers the render devices under `/dev/dri` of AMD and Intel GPUs, for Mesa's own drivers, on a host outside WSL. The Settings page shows it but does not change it, since the WSL package never uses it. |
| `GPU_ADAPTER` | Empty | The GPU to render on: in WSL a Mesa D3D12 adapter by vendor or part of its model name, with `/dev/dri` the `AMD` or `Intel` card. Empty leaves the choice to Mesa. |
| `AUDIO_ENABLED` | `true` | Allows containers to use browser sound. |
| `MICROPHONE_ENABLED` | `true` | Allows microphone input when audio is also enabled. |
| `INTERNET_ENABLED` | `true` | Allows containers internet access. `false` refuses it for new containers, in setting changes and on the internet switch. |
| `SUDO_ENABLED` | `true` | Allows containers to give the desktop user `sudo`. `false` refuses it for new containers and in setting changes. |
| `GAMEPAD_ENABLED` | `true` | Allows the virtual gamepad. `false` refuses it for new containers and in setting changes, and the manager passes no gamepad input. |
| `GPU_DEFAULT` | `true` | Whether a new container gets GPU access unless it asks otherwise, where the host has a GPU that `WSL_GPU`, `NVIDIA_COMPUTE` or `DRI_GPU` allows. |
| `INTERNET_DEFAULT` | `true` | Whether a new container gets internet access unless it asks otherwise. |
| `SUDO_DEFAULT` | `true` | Whether a new container gives the desktop user `sudo` unless it asks otherwise. |
| `AUDIO_DEFAULT` | `false` | Whether a new container gets sound unless it asks otherwise. |
| `MICROPHONE_DEFAULT` | `false` | Whether a new container with sound gets the microphone unless it asks otherwise. |
| `GAMEPAD_DEFAULT` | `false` | Whether a new container gets the virtual gamepad unless it asks otherwise. |
| `DESKTOP_USER_NAMESPACES` | `true` | Lets processes in a container create user namespaces, which Chrome, Edge, other Chromium-based browsers and Electron applications use for their sandbox. Containers get Docker's default seccomp profile with this one addition (`images/desktop/seccomp.json`); calls that need `CAP_SYS_ADMIN`, such as `mount`, stay blocked. Set `false` to keep Docker's default: those applications then start only with `--no-sandbox`, and opening a link in such a default browser fails. It applies when a container is created, so an existing container changes after a reset or a settings change that recreates it. |
| `UPLOAD_MAX_MB` | `1024` | Maximum file size in MiB, from 1 to 65536. |
| `IMAGE_ARCHIVE_MAX_MB` | `65536` | Largest image or volume archive an import accepts, in MiB after decompression, from 1 to 1048576. An image archive holds every layer of the image, so it is several gigabytes. |
| `DESKTOP_IMAGE` | XFCE GHCR image | The default image when creating a container. |
| `TEMPLATES_URL` | Empty | The template list the **Templates** page reads. Empty uses the list on the Bilskirnir wiki, `https://raw.githubusercontent.com/wiki/co3moz/bilskirnir/templates/index.json`. Set the `http` or `https` address of another `index.json` for your own list or a mirror, with each template's `<id>.json` and `<id>.png` beside it, or `none` to turn the page off. The browser reads the list, so it needs to reach that address; the manager does not. |
| `VOLUME_FILTER` | Empty | The Docker label a named volume needs to be listed, mounted, browsed, exported or copied, as `key` or `key=value`. Empty means `bilskirnir.managed=true`, which Bilskirnir puts on every volume it creates, imports or copies, so volumes made elsewhere on the same Docker Engine, such as the manager's own data volume or another Compose project's database, stay out of reach. A request that names such a volume is answered as if it did not exist. Bilskirnir adds the label you set here to the volumes it creates. A container keeps a volume it mounted before the filter left it out, through resets and setting changes; only a new choice is held to the filter. Docker cannot add a label to an existing volume, so to bring one in, set `none` for a while and copy it from the **Volumes** page. `none` lists every volume. |
| `UPDATE_CHECK` | `true` | Whether the manager asks GitHub for the newest release, when it starts, every six hours and when **Check now** on the **Settings** page asks. The sidebar names a release newer than the manager and links to its notes; in the [WSL package](WSL_PACKAGE.md#update-from-the-dashboard) its **Update** button runs `bilskirnir update`. `false` turns both off, and the manager makes no request to GitHub. |
| `ALLOWED_HOSTS` | `localhost` in Compose | Extra DNS names accepted by the manager, separated by commas. `DESKTOP_HOST` is accepted as well; Compose passes it on its own. |
| `SECURE_COOKIE` | `false` | Require HTTPS for the sign-in cookie. |

The full manifests also accept these Compose settings:

| Setting | Default | What it changes |
| --- | --- | --- |
| `BIND_ADDRESS` | `127.0.0.1` | Which host interfaces expose the manager and optional HTTPS service. |
| `PORT` | `8080` | The manager's HTTP port on the host. |
| `MANAGER_CONTAINER` | `bilskirnir-manager` | The manager container's name. |
| `MANAGER_DATA_VOLUME` | `bilskirnir-data` | The volume holding the manager database and temporary uploads. |
| `MANAGER_IMAGE` | `ghcr.io/co3moz/bilskirnir:latest` | The manager image in `compose.ghcr.yml`; accepts a version or digest. `latest` follows `master`; the WSL package sets the release it belongs to. |
| `DESKTOP_HOST` | `localhost` | The hostname for the optional HTTPS service; also added to accepted hosts. |
| `HTTPS_PORT` | `8443` | The optional HTTPS service's port on the host. |

Sessions last 12 hours. Signing out or reaching session expiry closes active desktop and media connections. Desktop containers continue running.

The container features of the create form have a switch and a default each. GPU access is on while the host has a GPU that `WSL_GPU`, `NVIDIA_COMPUTE` or `DRI_GPU` allows, which choose how a container gets it, so it has no switch of its own. The defaults are what the form starts with and what a request to `POST /api/containers` or the MCP `create_container` gets when it leaves a feature out; one the installation switched off is never a default, and the microphone is one only with sound. Before 0.6.3 such a request got no GPU access while the form chose it; `GPU_DEFAULT=true` gives requests what the form gives, so set it to `false` to keep the old answer. A switched-off feature is refused when a container is created or a setting change turns it on, and the form shows its chip turned off. A container that already has it keeps it in its settings, also when a reset or a settings change creates it again, as audio always worked: the manager only refuses what it carries itself while the feature is off, the audio and microphone channels and gamepad input. Turn internet or `sudo` off in such a container's settings to take it away. GPU access is the exception: while the host has no GPU for desktops, a reset, a retry or a settings change that creates a GPU container again is refused with the settings named, since its desktop would stop at the renderer check; turn GPU off in its settings to go on using it. Image builds keep their own internet setting, since templates install packages.

Containers created with the gamepad option receive the host's `/dev/uinput` device, which current WSL2 kernels provide; a root helper inside the desktop creates the virtual controller there.

Agent access uses a separate token rather than the dashboard session, unless `MCP_AUTH_MODE=none` turns it off. See the [MCP guide](MCP.md) to connect an agent, or the [HTTP API](API.md) to build another client.

### GPU on a Linux host

When it starts, the manager reads its host's `/sys`, which a container shares with the host, to see which GPUs desktops can have, and logs the routes GPU containers get. The Settings page shows the same in its **GPU** section, and `GET /api/settings` reports it in `gpu`. The manager sees the host only while it talks to a local Docker daemon; with a remote `DOCKER_HOST`, `WSL_GPU` and `NVIDIA_COMPUTE` apply as written and `/dev/dri` is never passed. Each route its setting allows is used where the host has it:

- **WSL GPU** (`WSL_GPU`): a WSL2 distribution's `/dev/dxg`, on which Mesa's D3D12 driver renders. There `NVIDIA_COMPUTE` adds CUDA beside it.
- **NVIDIA on a Linux host** (`NVIDIA_COMPUTE`): an NVIDIA GPU that NVIDIA's own driver runs, while Docker lists the NVIDIA Container Toolkit, as its `nvidia` runtime or an `nvidia.com/gpu` CDI device. Install the driver and the [toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html), then register it with `sudo nvidia-ctk runtime configure --runtime=docker` and restart Docker. GPU containers get the GPU with the toolkit's `graphics` capability, and the desktop image names NVIDIA's EGL library to glvnd, so OpenGL renders on NVIDIA's driver. Docker 29.2 and later hand `--gpus` to the toolkit's CDI devices where they exist, which carry the graphics libraries in any case.
- **AMD and Intel on a Linux host** (`DRI_GPU`, experimental): the render devices under `/dev/dri` of a GPU that NVIDIA's driver does not run. The desktop user joins groups with the devices' group IDs, and Mesa's own drivers render. `GPU_ADAPTER=AMD` or `Intel` picks that vendor's card on a host with both. Usage charts read `nvidia-smi`, so these GPUs have none.

A host without any of them gives desktops no GPU: new containers start without it, and a request for it is refused with each setting and what the host lacks. Only the WSL route has been verified on hardware, with an NVIDIA RTX 4080 in WSL2. The NVIDIA and `/dev/dri` routes on a Linux host follow the documented behavior of the toolkit, Mesa and VirtualGL but have not been run on such a host yet; the [development guide](DEVELOPMENT.md#gpu-on-a-linux-host) lists what to check there.

## Access from another device

For the minimal Compose example, change the port binding to `0.0.0.0:8080:8080`. For the full manifests, set `BIND_ADDRESS=0.0.0.0`. Open the manager using the host's LAN address.

The manager accepts localhost and private LAN IP addresses. If you use a DNS name, add it to `ALLOWED_HOSTS`. The host's firewall must also allow the connection, and under WSL so must the Windows firewall and the forwarding into WSL; Compose does not configure those host settings. The WSL package asks at first start whether other devices may connect and [describes the Windows side](WSL_PACKAGE.md#access-from-other-devices).

Keep login enabled when other people can reach the manager. Its Docker socket connection gives it control of the host Engine. The current application has one administrator, with no separate accounts or per-user container permissions.

## HTTPS and microphone access over LAN

Use the full manifest for the optional Caddy HTTPS service. It creates a local certificate authority, which each client device needs to trust. This also lets a remote browser request microphone access.

Set these values in `.env`, using a hostname that points to your host (the Windows machine, under WSL):

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

The sidebar names a newer release when one is out, unless `UPDATE_CHECK=false`. Only the WSL package updates itself from there; on other installations, run the commands above. `compose.ghcr.yml` mounts an `update` folder beside it into the manager for that, which stays empty elsewhere.

This updates the manager. Catalog images are not Compose services: the manager downloads one the first time a container is created from it. To get a newer release of a catalog image you already have, select the update button on its row under **Images**. A release manager, such as `ghcr.io/co3moz/bilskirnir:1.2.0`, downloads the catalog image published with it (`ubuntu-xfce:1.2.0`) and tags it with the catalog reference (`ubuntu-xfce:24.04`); a manager built from `master`, such as `latest`, downloads the reference itself, which `master` moves. `docker pull` with the reference therefore always gives the `master` build. In the [WSL package](WSL_PACKAGE.md#update), `bilskirnir update` does both and moves the manager to the newest release, together with that release's `compose.yml` and `bilskirnir` command.

Compose files from earlier releases list two short-lived `desktop` and `desktop-gnome` services that download both catalog images at every start. They keep working. To download them only when they are used, replace the file with the current `compose.ghcr.yml` (or remove those services and the manager's `depends_on`), then run `docker compose up -d --remove-orphans` to remove their exited containers. Images already downloaded stay in place.

New containers use the updated catalog images. Existing containers and snapshots keep their saved image IDs; updating the manager does not reinstall their applications or replace their files. Keep the manager data volume when updating. To stay on a particular build, pin image references to a published version, `sha-<full-commit>` tag or digest.

## Know where your data lives

| Data | Location | Effect of deleting a container |
| --- | --- | --- |
| Container settings, image records and jobs | `/data/state.sqlite` in the manager data volume | The manager's database remains. |
| Installed software and unmounted home files | The desktop container's writable layer | Lost unless saved in an image or snapshot. |
| Files carried over by a settings change | An image tagged `bilskirnir/container:JOB_ID` that the container runs | Removed, unless a snapshot builds on it. |
| Files in named-volume mounts | Separate Docker volumes | Preserved. |
| Saved image and snapshot layers | The host Docker Engine's image store | Preserved. |

A snapshot alone is not a backup of the whole installation: it excludes volumes and the manager database. A backup from **Settings** is one. It carries the manager's records and, as you choose, the saved images, each container's files and the named volumes; [moving to another computer](#move-to-another-computer) describes it. The settings in `.env` and the catalog images stay out of it.

Uploads are temporarily staged in the manager data volume. The manager accepts two transfers at once, each with a one-hour limit. Manager-side temporary upload files are removed after completion or cancellation, and leftover staging files are cleaned on startup. Make room for staging as well as the destination files.

## When setup is interrupted

A manager restart marks unfinished jobs as interrupted. Build scripts are not replayed automatically. Inspect the job result before starting an operation again.

Use **Retry setup** for a container that failed during creation or startup. If its container still exists, Bilskirnir keeps the writable layer and completes missing networks or startup steps. If the container is missing, it can recreate it from the saved source image ID.

A settings change that needs a new container saves its files to an image and renames the old one to `bilskirnir-ID-previous` until its replacement has started. If the manager stops during that job, the old container and its files are still there under that name, and the container record names the saved image. **Retry setup** then creates the container from that image with the new settings and removes the one set aside, as does the next settings change; a reset or a delete removes it with the container. Only a record written by a manager before 0.5.0 lacks the saved image, and then Retry setup and settings changes refuse to run while `bilskirnir-ID-previous` exists: rename it back with `docker rename bilskirnir-ID-previous bilskirnir-ID`, or remove it once its files are no longer needed.

Recreating a missing container cannot restore changes that were never saved as a snapshot. Missing images or volumes stop recovery so you can resolve them; Bilskirnir does not silently replace a missing volume with an empty one.

The manager checks container network connections every 20 seconds. A problem with one container does not prevent checks for the others. Changes made directly through Docker can still conflict with manager operations, so inspect affected resources before trying recovery.

## If something does not start

If the manager is not reachable, check its startup log:

```bash
docker compose logs --tail=100 manager
```

Use `-f compose.ghcr.yml` with the full registry manifest. A missing or too-short password prevents startup when `AUTH_MODE=password`. A registry error happens before startup; check that the host can reach GHCR.

If the dashboard opens but a container fails, read its job log and **Logs**; **Diagnostics** needs a running desktop. For a GPU error, the job log names the setting behind a device Docker refused, and a desktop that stops at its renderer check names the one to check in **Logs**; [GPU on a Linux host](#gpu-on-a-linux-host) explains each route. You can create a separate container with GPU disabled to check the basic desktop path without deleting the failed container's files.

If file uploads report a permission error, check that the chosen volume is writable by UID/GID 1001:1001. Raising the upload size limit will not fix destination permissions.

## Move to another computer

A backup moves an installation, A, to another computer or a fresh installation, B. What it carries and how it streams are described in [the API guide](API.md#backup-and-restore).

1. On A, open **Settings** and, under **Backup and restore**, tick **Saved images**, **Containers' files** and **Volumes** to move everything. **Secret values** adds the values of the composes' secret parameters; without them, a reset on B runs those setups without the values. Select **Download backup**. Containers may keep running; Docker pauses each desktop briefly while it saves its files.
2. Copy the file to B. It is not encrypted. It holds your files, the hashes of the MCP endpoints' tokens and, when ticked, secret values in plain text, so keep it where only you can read it, encrypt it with a tool such as `gpg --symmetric` or 7-Zip before it crosses a network share, and delete it once B works. Nothing is staged on either manager's disk: A streams the backup into your download, and B loads it into Docker as it uploads, so B needs room for the images, files and volumes themselves.
3. Install Bilskirnir on B with a new data volume. Settings do not travel in the backup, since they live in `.env` outside the manager. In the WSL package, run `bilskirnir settings` on B and copy the values you changed in A's `/opt/bilskirnir/.env`, such as `ADMIN_PASSWORD`, `MCP_TOKEN` and `BIND_ADDRESS`, then `bilskirnir restart`; with Compose, copy `.env` and recreate the manager. A restore reads at most `IMAGE_ARCHIVE_MAX_MB` (64 GiB by default) after decompression, so raise it on B for a larger backup.
4. On B, choose the file under **Restore a backup**. The dashboard lists what it creates and anything B already has under the same name; a restore never replaces anything, so a conflict stops it before it starts. Select **Restore** and keep the tab open until the upload ends. The job then saves the records and creates the containers.
5. The containers come back stopped, with the same hostname, MAC address and machine ID, so node-locked licenses stay valid. Start them from **Containers**. A container restored without its files, or one that mounts a volume the backup did not carry, shows why. **Retry setup** creates the first from its image, downloading a catalog image when B has none yet, and runs its compose setup again, and the second once its volume is on B. Agents keep their endpoint tokens. While A stays in use, the same tokens open both installations, so give the endpoints on one side new ones with **Replace token** on the **MCP** page.

## Move an older installation

Before replacing an older Compose deployment, set `MANAGER_DATA_VOLUME` to its existing manager data volume name. The current default is `bilskirnir-data`. Remove the old manager deployment without deleting that volume, then start Bilskirnir using it. Existing containers can keep running while the manager is replaced.

If locally built desktop images still use older tags, retag them rather than rebuilding:

```bash
docker image tag bilskirnir/ubuntu-xfce:24.04 ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04
docker image tag bilskirnir/ubuntu-gnome:24.04 ghcr.io/co3moz/bilskirnir/ubuntu-gnome:24.04
```

Saved container image IDs do not change when you add a tag.
