# Installing Bilskirnir as a WSL distribution

[Back to the README](../README.md)

The `bilskirnir.wsl` package is a ready-made WSL distribution: Ubuntu 24.04 with Docker Engine, Compose, Buildx, the NVIDIA Container Toolkit and the Bilskirnir installation files. Installing it replaces the "install Ubuntu, install Docker, configure the GPU, prepare Compose" steps with one command. The distribution runs its own Docker Engine, with images and containers separate from Docker in any other WSL distribution; only the network is shared, see [Docker in other WSL distributions](#docker-in-other-wsl-distributions).

The package contains no application images. The first start downloads the manager from GHCR, and each catalog image downloads the first time a container is created from it. This keeps the package small, leaves images you never use off the disk and lets `bilskirnir update` fetch newer images later.

Each package installs the manager release it belongs to. The package from release 1.2.0 writes `MANAGER_IMAGE=ghcr.io/co3moz/bilskirnir:1.2.0` to `.env`, so installing it months later still gives you manager 1.2.0, however many releases came out since, and that manager downloads the catalog images published with 1.2.0. Only [`bilskirnir update`](#update) moves it forward.

## Requirements

- Windows 11 or Windows 10 with WSL 2.4.4 or newer (`wsl --version`), updated with `wsl --update`.
- For GPU containers, a Windows GPU driver with WSL support. The NVIDIA Windows driver provides CUDA inside WSL; no Linux display driver is installed in the package. See [NVIDIA's WSL guide](https://docs.nvidia.com/cuda/wsl-user-guide/index.html).
- About 2 GB of disk space for the distribution, plus the manager image, each catalog image you use (XFCE about 1.2 GB, GNOME about 2.3 GB, sharing their base layers) and your containers.

## Install

The package installs in one of two ways. Both give the same distribution, and the Windows helper works with one installed either way.

### Option 1: the Windows helper

Bilskirnir's PowerShell helper, `bilskirnir.ps1`, is published beside the package in every release and runs straight from there. In PowerShell:

```powershell
irm https://github.com/co3moz/bilskirnir/releases/latest/download/bilskirnir.ps1 | iex
```

`irm` downloads the helper and `iex` runs it, so nothing is saved and no script policy is in the way. While Bilskirnir is not installed, it asks for a folder for the distribution's disk, which holds every image, container and volume (Enter keeps it under `%LOCALAPPDATA%\wsl`), and for the disk's maximum size (Enter for 1 TB, see [Disk](#disk)). It then downloads `bilskirnir.wsl` from the latest release, registers the **Bilskirnir** distribution and runs its first start in the same window. Run the line again later for [everything else it does](#the-windows-helper).

### Option 2: the package by hand

WSL installs the package by itself as well. Download it, then double-click it or run the second line; `--location D:\WSL\Bilskirnir` and `--vhd-size 200GB` choose the disk's folder and maximum size:

```powershell
curl.exe -fLo bilskirnir.wsl https://github.com/co3moz/bilskirnir/releases/latest/download/bilskirnir.wsl
wsl --install --from-file .\bilskirnir.wsl
```

Then open the new **Bilskirnir** entry from the Start menu or run `wsl -d Bilskirnir` for the first start.

### First start

The first start asks for one username and password. They create the Linux user and are also the dashboard sign-in; an empty password leaves the Linux user without one and turns dashboard sign-in off (`AUTH_MODE=none`). A password needs at least eight characters.

Next it asks whether to enable agent access (MCP); move between the answers with the up and down arrows and confirm with Enter. **Yes** is the default. With agent access on, it asks how agents authenticate:

- **Random token** (the default) generates a 64-character token.
- **Custom token** takes a token you type: 1 to 256 visible ASCII characters without spaces or single quotes. It is stored in single quotes in `.env`, so `$` and `#` stay literal. A short token is easy to guess, and whoever has it controls every container.
- **None** sets `MCP_AUTH_MODE=none`: agents connect without a token, and anything that can reach the dashboard address controls every container.

The setup then detects the WSL GPU device and CUDA libraries, writes the settings to `/opt/bilskirnir/.env` and asks whether to start Bilskirnir now, with the same kind of menu and **Yes** as the default. Starting downloads the manager and brings it up; the dashboard is then at [localhost:8080](http://localhost:8080). With agent access on, the setup ends by printing the agent endpoint and client settings, which `bilskirnir mcp` shows again later.

A first start with piped input, such as an automated installation, answers each menu with a line holding the answer's first word (`yes`, `no`, `random`, `custom`, `none`) or its first letter; an empty line or the end of the input picks the default. It is not asked whether to start, and leaves that to `bilskirnir start`.

## The Windows helper

The helper handles what has to happen outside the distribution. Run it with the same line as for installing; with Bilskirnir installed, it shows whether Bilskirnir is running and where its disk is, then a numbered list:

| Entry | Command | What it does |
| --- | --- | --- |
| Start it and keep it running | `start` | Boots the distribution, keeps it running and starts the manager, see [Keep it running](#keep-it-running). |
| Stop it | `stop` | Stops the manager and shuts the distribution down. |
| Show the manager state | `status` | Runs `bilskirnir status`. |
| Open the dashboard | `open` | Opens the dashboard in the browser. |
| Update it to the newest release | `update` | Runs [`bilskirnir update`](#update). |
| Move its disk to another folder | `move <folder>` | Moves the disk, see [Disk](#disk). |
| Change its disk's maximum size | `resize <size>` | Changes the disk's maximum size, see [Disk](#disk). |
| Start it at every Windows sign-in | `autostart enable` | Registers the auto-start task; with the task registered, the entry removes it instead (`autostart disable`). |
| Remove it with its images, containers and volumes | `uninstall` | Removes the auto-start task and, after confirmation, the distribution, see [Remove](#remove). |

`irm | iex` cannot pass a command. To run one without the list, for example from a script, make the helper a script block and pass the command after it:

```powershell
& ([scriptblock]::Create((irm https://github.com/co3moz/bilskirnir/releases/latest/download/bilskirnir.ps1))) move D:\WSL\Bilskirnir
```

`install` takes `-Location <folder>`, `-VhdSize <size>` and `-Package <file>`, a package you already have. Every command takes `-Distro <name>` for a distribution installed under another name and `-Port <port>` for a dashboard port other than the one in `.env`, and `help` lists them all. A saved copy takes the same commands, run as `powershell -ExecutionPolicy Bypass -File .\bilskirnir.ps1 <command>` because Windows runs a saved script only when allowed to; in a clone of the repository it is `windows\bilskirnir.ps1`.

## Keep it running

WSL stops a distribution shortly after its last Windows-started process ends, even though Docker runs as a systemd service inside it. Two ways keep Bilskirnir available:

- The helper's `start` boots the distribution, keeps a hidden keep-alive process open and starts the manager.
- Its `autostart enable` registers a scheduled task that runs the keep-alive process at Windows sign-in. Docker and the manager start with the distribution because the manager container uses `restart: unless-stopped`.

The helper's `stop` stops the manager and shuts the distribution down.

## Everyday commands

Inside the distribution, `bilskirnir` manages the installation:

| Command | What it does |
| --- | --- |
| `bilskirnir start` | Starts Docker if needed, downloads the manager if it is missing and starts it. |
| `bilskirnir status` | Shows the Compose services and the dashboard address. |
| `bilskirnir logs` | Shows the manager's recent log. |
| `bilskirnir update` | Moves the manager, its `compose.yml` and the `bilskirnir` command to the newest release, pulls the catalog images already downloaded, then recreates the manager. |
| `bilskirnir settings` | Opens `/opt/bilskirnir/.env` through `sudoedit` in your editor (`VISUAL` or `EDITOR`, nano by default); run `bilskirnir restart` afterwards. |
| `bilskirnir doctor` | Checks systemd, Docker, its firewall rules, memory, processors, free disk space, the GPU device, CUDA libraries, settings and registry reachability. |
| `bilskirnir open` | Opens the dashboard in the Windows browser. |
| `bilskirnir mcp` | Shows the agent endpoint, its token and ready-to-paste client settings. |

Tab completes the commands: type `bilskirnir re` and press Tab for `restart`. The package installs `bash-completion` for it, so `docker` completes as well. Packages before 0.6.0 have neither; an update does not add them.

The installation files live in `/opt/bilskirnir`: `compose.yml` (the published-image manifest), `.env`, `.env.example` and `deploy/Caddyfile`. [`bilskirnir update`](#update) keeps all but `.env` in step with the manager. Everything in the [installation guide](INSTALLATION.md) applies, including LAN access and HTTPS; run the Compose commands from `/opt/bilskirnir`.

## Update

`bilskirnir update` looks up the newest release on GitHub, downloads its manager, writes it to `MANAGER_IMAGE` in `.env` and recreates the manager. It never moves to a release older than the installed one, and leaves `.env` as it was when the lookup or the download fails. In the same step it downloads the catalog images you already have again, in the version the new manager uses: a release manager takes the images published with it, such as `ubuntu-xfce:1.2.0`, and tags them with their usual catalog reference, `ubuntu-xfce:24.04`, which containers and images record.

A `MANAGER_IMAGE` set by hand to anything other than a release of `ghcr.io/co3moz/bilskirnir`, such as `latest`, which follows `master`, or a digest, is only pulled again. To return to releases, set it to one with `bilskirnir settings`, for example `ghcr.io/co3moz/bilskirnir:1.2.0`, and run `bilskirnir update`.

Before it recreates the manager, an update copies the installation files that manager bundles, `compose.yml`, `deploy/Caddyfile` and `.env.example`, into `/opt/bilskirnir`. A setting added in a release reaches the manager only through the `environment:` list in that release's `compose.yml`, so with an older file, setting it in `.env` would do nothing. `.env` is never replaced; compare it with the new `.env.example` to find the settings a release added.

The package and every update record a checksum of each of these files in `/opt/bilskirnir/.shipped.sha256`, so an update can tell whether you edited one:

- A file you have not edited is replaced.
- A file you edited stays as it is when the release brings the same version of it as before.
- A file you edited is saved as `<file>.bak`, for example `compose.yml.bak`, when the release brings a new version, and is then replaced; the update names the file it saved. Carry over the changes you still need. The manager starts with the new file right away, so a change such as a different port mapping is missing until you carry it over and run `bilskirnir restart`. An earlier `.bak` is replaced only when you edit the file again.

To keep your own Compose changes out of the way of updates, put them in `/opt/bilskirnir/compose.override.yml` rather than editing `compose.yml`. Compose merges that file into `compose.yml` on its own, and updates never touch it.

### The bilskirnir command

The manager image also bundles the `bilskirnir` command of its release, so fixes to `update`, `doctor` or `mcp` reach an installation without a new package. Before it copies the installation files, an update replaces `/usr/local/bin/bilskirnir` with the command of the release it moves to, but only when all of these hold:

- `MANAGER_IMAGE` is a release of `ghcr.io/co3moz/bilskirnir`, as the package sets it. A manager you chose by hand, such as `latest`, a digest or a fork, already runs with access to Docker, but as a container that the next change of `MANAGER_IMAGE` replaces. A command it brought would stay after you switch back, and would keep running as you with passwordless `sudo`. With such a manager, the update says the command stays as it is.
- The release is newer than the one the command came with. The command never moves back, not even when you set an older manager by hand.
- The distribution has the package files the new command relies on. The package records the level of its own files, such as the firewall watcher, in `/usr/lib/wsl/package-level`, and every command names the level it needs. When a release needs a newer package, the update says so, keeps the command and updates everything else. Only a newer package brings that command, and installing one replaces the distribution together with its Docker data, see [Remove](#remove).
- The new command is complete: it names the level it needs and passes a syntax check.

The new command is written beside the old one and renamed over it, so an interrupted update leaves one of the two whole. The update then starts over with the new command, which copies the installation files by its release's rules and recreates the manager. Changes you made to the command itself are not kept. The rest of the distribution, such as Docker, the firewall watcher and the first-start setup, stays as the package installed it.

The command of the 0.3.0 package and of earlier ones cannot update itself, and those packages set `MANAGER_IMAGE` to `latest`. Install the command of a newer release once by hand, replacing `1.2.0` with it:

```bash
docker run --rm --entrypoint cat ghcr.io/co3moz/bilskirnir:1.2.0 /app/install/bilskirnir > /tmp/bilskirnir
```

```bash
sudo install -m 0755 /tmp/bilskirnir /usr/local/bin/bilskirnir
```

Then set `MANAGER_IMAGE` to that release, as described above, and run `bilskirnir update`. From then on, updates bring the command.

## Memory, processors and disk

Every WSL distribution runs in one shared virtual machine. Its memory and processor count are Windows settings for all distributions together, so the package cannot set them. The disk is different: each distribution has its own virtual disk, sized separately.

### Memory and processors

Unless configured otherwise, WSL gives the virtual machine half of the Windows memory and all of its processors, and Bilskirnir shares them with every other running distribution. Change them in `%UserProfile%\.wslconfig` or in the **WSL Settings** app:

```ini
[wsl2]
memory=24GB
processors=12
swap=8GB
```

WSL reads these settings only when the virtual machine starts, and the helper's `stop` stops only this distribution. Apply them with `wsl --shutdown`, which stops every WSL distribution, then start Bilskirnir again with the helper's `start`:

```powershell
wsl --shutdown
```

The manager comes back on its own; containers do not, so start them again from the dashboard. Their running applications and memory are lost, as with any stop. Afterwards the dashboard's **MEMORY** figure and `bilskirnir doctor` show the new amount, slightly below the configured value because the kernel keeps part of it. `bilskirnir doctor` warns when it is below 8 GB.

A container without a memory limit can use all of the virtual machine's memory. When that runs out, Linux ends processes inside the virtual machine, which can include the manager or another container; Windows itself stays protected by the limit above. Give containers that run heavy applications a memory limit when you [create them](USAGE.md#create-and-connect) or change their settings. A limit is a ceiling, not a reservation: memory a container does not use stays available to the rest.

### Disk

The distribution's virtual disk grows as it fills, up to a maximum of 1 TB unless chosen otherwise. Images, snapshots, containers and volumes all live in it, so the Windows drive that holds the disk needs the space too. Choose the folder and the maximum when installing: the helper asks for both, and a [package installed by hand](#option-2-the-package-by-hand) takes `--location` and `--vhd-size`.

To change them later, choose **Move its disk to another folder** or **Change its disk's maximum size** in the [helper](#the-windows-helper), or give it the command directly:

```powershell
& ([scriptblock]::Create((irm https://github.com/co3moz/bilskirnir/releases/latest/download/bilskirnir.ps1))) move D:\WSL\Bilskirnir
& ([scriptblock]::Create((irm https://github.com/co3moz/bilskirnir/releases/latest/download/bilskirnir.ps1))) resize 200GB
```

WSL changes a disk only while nothing has it attached, and it keeps a stopped distribution's disk attached until the WSL virtual machine shuts down. So both stop the manager and run `wsl --shutdown`, which stops every WSL distribution; when others are running, they name them and ask first. Then they change the disk and start Bilskirnir again. The manager comes back on its own; containers do not, so start them again from the dashboard. A move to another drive copies the whole disk, which takes a while for a large one, and the folder must not hold a disk already.

A smaller maximum works only while the data still fits. `bilskirnir doctor` shows the space left before the maximum and warns below 10 GB.

The disk file does not shrink when images or containers are deleted. Two ways return freed space to Windows, and they exclude each other: run `wsl --manage Bilskirnir --set-sparse true` once, which WSL still labels experimental, or run `wsl --manage Bilskirnir --compact` from time to time. Both need the disk released as above: run the helper's `stop` and `wsl --shutdown` first and its `start` afterwards. A sparse disk cannot be compacted.

## What the package decides for you

- The Linux user created at first start has passwordless `sudo` and belongs to the `docker` group, and its name is the dashboard's `ADMIN_USERNAME`. The distribution is a single-purpose appliance for the Windows user who installed it.
- `.env` holds the dashboard password and the agent token, so it belongs to root and only the `docker` group may read it (`root:docker`, mode `0640`). `bilskirnir settings` saves it through `sudoedit`, which keeps that owner and mode; change it any other way with `sudo`.
- Without a password, the dashboard has no sign-in. That is safe only while it listens on localhost; set `AUTH_MODE=password` and an `ADMIN_PASSWORD` in `.env` before allowing LAN access.
- Docker runs with the NVIDIA runtime registered. `WSL_GPU` is set when `/dev/dxg` and the WSL libraries exist, and `NVIDIA_COMPUTE` when `libcuda` is present; otherwise containers run without GPU acceleration. Change these in `.env` if the detection is wrong.
- Agent access is enabled unless the first start turned it off: `MCP_ENABLED=true` with a random 64-character `MCP_TOKEN` by default, a custom token or `MCP_AUTH_MODE=none`. `bilskirnir mcp` prints the endpoint, the token and the [client settings](MCP.md); agent access grants administrator access to every container, so set `MCP_ENABLED=false` in `.env` and restart if no agent will use this installation. Without a token it is safe only while the manager listens on localhost.
- The manager listens on `127.0.0.1:8080`. For LAN access set `BIND_ADDRESS=0.0.0.0` in `.env`, allow the port through the Windows firewall and forward it into WSL as described in the installation guide.

## Docker in other WSL distributions

WSL 2 runs every distribution in one network namespace. Docker Engines in different distributions keep their own images and containers, but they write their NAT, port and isolation rules into the same firewall, and none of them notices when another distribution removes its rules:

- An enabled `nftables` service removes every rule twice: the default `/etc/nftables.conf` it loads at boot starts with `flush ruleset`, and stopping it at shutdown runs `nft flush ruleset`. Containers stop reaching the internet even with internet access on, and creating a container fails with `iptables: No chain/target/match by that name`. Installing Docker Engine on Ubuntu 24.04 brings in that package with the service enabled.
- A Docker Engine starting in another distribution rebuilds Docker's shared chains with only its own networks. Internet access keeps working, but the isolation between networks does not.

The package masks the `nftables` service, and `bilskirnir doctor` reports it when an installation from an older package still has it enabled. Mask it in the other distributions that run Docker too:

```bash
sudo systemctl mask nftables
```

The `bilskirnir-firewall` service repairs the damage when the rules disappear anyway. Every 10 seconds it checks that each Docker network still has its rule in Docker's `DOCKER-FORWARD` chain; when one is missing twice in a row, it restarts Docker. Docker runs with `live-restore`, so containers and the manager keep running, but an operation talking to Docker at that moment, such as a script whose output is being streamed, can fail. `journalctl -u bilskirnir-firewall` lists the repairs and `bilskirnir doctor` shows the current state.

## Build the package

The package is produced from `wsl/Dockerfile` with Docker:

```bash
scripts/build-wsl.sh dist/bilskirnir.wsl
```

The script builds the root filesystem image, exports it with `docker export` and compresses it with `xz -9e`, which makes the package about 40% smaller than gzip; `WSL_COMPRESS=gzip` builds faster but produces a larger file. The package installs the manager version in `Cargo.toml`; `BILSKIRNIR_VERSION=1.2.0` builds one for another published release, and `BILSKIRNIR_VERSION=latest` one that follows `master`. It refuses to package a world-writable file, which a `COPY` without `--chmod` produces from a checkout on a Windows drive, or a directory its owner cannot enter, which `COPY --chmod=0644` produces when it creates the destination directory. Release tags publish the package and the Windows helper, `windows/bilskirnir.ps1`, as GitHub release assets once the tag's images are published. The files under `wsl/` define the distribution: `wsl.conf` enables systemd, `wsl-distribution.conf` names the distribution and points WSL at the first-start script, `oobe.sh` performs that setup, `bilskirnir` is the management command, `keepalive.sh` is the process the Windows launcher keeps open, `daemon.json` configures Docker and `firewall-watch.sh` with `bilskirnir-firewall.service` restores Docker's firewall rules. The image masks `systemd-firstboot`, which would otherwise wait on the console for a machine ID and block every other unit, `kmod-static-nodes`, which has no module tree in WSL, and `nftables`, which flushes the firewall shared by all distributions; it also installs the systemd user-session packages so launches do not warn about a missing user session.

## Remove

The helper's **Remove it with its images, containers and volumes** (`uninstall`) removes the auto-start task and, after confirmation, unregisters the distribution. Unregistering deletes its Docker images, containers and named volumes, so export anything you want to keep first.
