# Installing Bilskirnir as a WSL distribution

[Back to the README](../README.md)

The `bilskirnir.wsl` package is a ready-made WSL distribution: Ubuntu 24.04 with Docker Engine, Compose, Buildx, the NVIDIA Container Toolkit and the Bilskirnir installation files. Installing it replaces the "install Ubuntu, install Docker, configure the GPU, prepare Compose" steps with one command. The distribution runs its own Docker Engine, with images and containers separate from Docker in any other WSL distribution; only the network is shared, see [Docker in other WSL distributions](#docker-in-other-wsl-distributions).

The package contains no application images. The first start downloads the manager from GHCR, and each desktop profile downloads the first time a desktop is created from it. This keeps the package small, leaves profiles you never use off the disk and lets `bilskirnir update` fetch newer images later.

Each package installs the manager release it belongs to. The package from release 1.2.0 writes `MANAGER_IMAGE=ghcr.io/co3moz/bilskirnir:1.2.0` to `.env`, so installing it months later still gives you manager 1.2.0, however many releases came out since, and that manager downloads the desktop profiles published with 1.2.0. Only [`bilskirnir update`](#update) moves it forward.

## Requirements

- Windows 11 or Windows 10 with WSL 2.4.4 or newer (`wsl --version`), updated with `wsl --update`.
- For GPU desktops, a Windows GPU driver with WSL support. The NVIDIA Windows driver provides CUDA inside WSL; no Linux display driver is installed in the package. See [NVIDIA's WSL guide](https://docs.nvidia.com/cuda/wsl-user-guide/index.html).
- About 2 GB of disk space for the distribution, plus the manager image, each desktop profile you use (XFCE about 1.2 GB, GNOME about 2.3 GB, sharing their base layers) and your desktops.

## Install

Download `bilskirnir.wsl` from a release, then either double-click it or run:

```powershell
wsl --install --from-file .\bilskirnir.wsl
```

Open the new **Bilskirnir** entry from the Start menu or run `wsl -d Bilskirnir`. The first start asks for one username and password. They create the Linux user and are also the dashboard sign-in; an empty password leaves the Linux user without one and turns dashboard sign-in off (`AUTH_MODE=none`). A password needs at least eight characters.

Next it asks whether to enable agent access (MCP); move between the answers with the up and down arrows and confirm with Enter. **Yes** is the default. With agent access on, it asks how agents authenticate:

- **Random token** (the default) generates a 64-character token.
- **Custom token** takes a token you type: 1 to 256 visible ASCII characters without spaces or single quotes. It is stored in single quotes in `.env`, so `$` and `#` stay literal. A short token is easy to guess, and whoever has it controls every desktop.
- **None** sets `MCP_AUTH_MODE=none`: agents connect without a token, and anything that can reach the dashboard address controls every desktop.

The setup then detects the WSL GPU device and CUDA libraries, writes the settings to `/opt/bilskirnir/.env` and asks whether to start Bilskirnir now, with the same kind of menu and **Yes** as the default. Starting downloads the manager and brings it up; the dashboard is then at [localhost:8080](http://localhost:8080). With agent access on, the setup ends by printing the agent endpoint and client settings, which `bilskirnir mcp` shows again later.

A first start with piped input, such as an automated installation, answers each menu with a line holding the answer's first word (`yes`, `no`, `random`, `custom`, `none`) or its first letter; an empty line or the end of the input picks the default. It is not asked whether to start, and leaves that to `bilskirnir start`.

The bundled PowerShell helper does the same from Windows and adds auto-start:

```powershell
.\windows\bilskirnir.ps1 install -Package .\bilskirnir.wsl
.\windows\bilskirnir.ps1 autostart enable
```

## Keep it running

WSL stops a distribution shortly after its last Windows-started process ends, even though Docker runs as a systemd service inside it. Two ways keep Bilskirnir available:

- `.\windows\bilskirnir.ps1 start` boots the distribution, keeps a hidden keep-alive process open and starts the manager.
- `.\windows\bilskirnir.ps1 autostart enable` registers a scheduled task that runs the keep-alive process at Windows sign-in. Docker and the manager start with the distribution because the manager container uses `restart: unless-stopped`.

`.\windows\bilskirnir.ps1 stop` stops the manager and shuts the distribution down.

## Everyday commands

Inside the distribution, `bilskirnir` manages the installation:

| Command | What it does |
| --- | --- |
| `bilskirnir start` | Starts Docker if needed, downloads the manager if it is missing and starts it. |
| `bilskirnir status` | Shows the Compose services and the dashboard address. |
| `bilskirnir logs` | Shows the manager's recent log. |
| `bilskirnir update` | Moves the manager to the newest release, pulls the desktop profiles already downloaded, then recreates the manager. |
| `bilskirnir settings` | Opens `/opt/bilskirnir/.env`; run `bilskirnir restart` afterwards. |
| `bilskirnir doctor` | Checks systemd, Docker, its firewall rules, memory, processors, free disk space, the GPU device, CUDA libraries, settings and registry reachability. |
| `bilskirnir open` | Opens the dashboard in the Windows browser. |
| `bilskirnir mcp` | Shows the agent endpoint, its token and ready-to-paste client settings. |

The installation files live in `/opt/bilskirnir`: `compose.yml` (the published-image manifest), `.env` and `deploy/Caddyfile`. Everything in the [installation guide](INSTALLATION.md) applies, including LAN access and HTTPS; run the Compose commands from `/opt/bilskirnir`.

## Update

`bilskirnir update` looks up the newest release on GitHub, downloads its manager, writes it to `MANAGER_IMAGE` in `.env` and recreates the manager. It never moves to a release older than the installed one, and leaves `.env` as it was when the lookup or the download fails. In the same step it downloads the desktop profiles you already have again, in the version the new manager uses: a release manager takes the images published with it, such as `ubuntu-xfce:1.2.0`, and tags them with the profile's usual reference, `ubuntu-xfce:24.04`, which desktops and images record.

A `MANAGER_IMAGE` set by hand to anything other than a release of `ghcr.io/co3moz/bilskirnir`, such as `latest`, which follows `master`, or a digest, is only pulled again. To return to releases, set it to one with `bilskirnir settings`, for example `ghcr.io/co3moz/bilskirnir:1.2.0`, and run `bilskirnir update`.

An update replaces images only. `compose.yml`, `.env.example` and the `bilskirnir` command stay as the package installed them.

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

WSL reads these settings only when the virtual machine starts, and `bilskirnir.ps1 stop` stops only this distribution. Apply them with `wsl --shutdown`, which stops every WSL distribution, then start Bilskirnir again:

```powershell
wsl --shutdown
.\windows\bilskirnir.ps1 start
```

The manager comes back on its own; desktops do not, so start them again from the dashboard. Their running applications and memory are lost, as with any stop. Afterwards the dashboard's **MEMORY** figure and `bilskirnir doctor` show the new amount, slightly below the configured value because the kernel keeps part of it. `bilskirnir doctor` warns when it is below 8 GB.

A desktop without a memory limit can use all of the virtual machine's memory. When that runs out, Linux ends processes inside the virtual machine, which can include the manager or another desktop; Windows itself stays protected by the limit above. Give desktops that run heavy applications a memory limit when you [create them](USAGE.md#create-and-connect) or change their settings. A limit is a ceiling, not a reservation: memory a desktop does not use stays available to the rest.

### Disk

The distribution's virtual disk grows as it fills, up to a maximum of 1 TB unless chosen otherwise. Images, snapshots, desktops and volumes all live in it, so the Windows drive that holds the disk needs the space too. Choose the location and the maximum when installing:

```powershell
wsl --install --from-file .\bilskirnir.wsl --location D:\WSL\Bilskirnir --vhd-size 200GB
```

To change them later, stop the distribution first; other distributions can keep running:

```powershell
.\windows\bilskirnir.ps1 stop
wsl --manage Bilskirnir --resize 200GB
wsl --manage Bilskirnir --move D:\WSL\Bilskirnir
.\windows\bilskirnir.ps1 start
```

A smaller maximum works only while the data still fits. `bilskirnir doctor` shows the space left before the maximum and warns below 10 GB.

The disk file does not shrink when images or desktops are deleted. Two ways return freed space to Windows, and they exclude each other: run `wsl --manage Bilskirnir --set-sparse true` once, which WSL still labels experimental, or run `wsl --manage Bilskirnir --compact` on the stopped distribution from time to time. A sparse disk cannot be compacted.

## What the package decides for you

- The Linux user created at first start has passwordless `sudo` and belongs to the `docker` group, and its name is the dashboard's `ADMIN_USERNAME`. The distribution is a single-purpose appliance for the Windows user who installed it.
- Without a password, the dashboard has no sign-in. That is safe only while it listens on localhost; set `AUTH_MODE=password` and an `ADMIN_PASSWORD` in `.env` before allowing LAN access.
- Docker runs with the NVIDIA runtime registered. `WSL_GPU` is set when `/dev/dxg` and the WSL libraries exist, and `NVIDIA_COMPUTE` when `libcuda` is present; otherwise desktops run without GPU acceleration. Change these in `.env` if the detection is wrong.
- Agent access is enabled unless the first start turned it off: `MCP_ENABLED=true` with a random 64-character `MCP_TOKEN` by default, a custom token or `MCP_AUTH_MODE=none`. `bilskirnir mcp` prints the endpoint, the token and the [client settings](MCP.md); agent access grants administrator access to every desktop, so set `MCP_ENABLED=false` in `.env` and restart if no agent will use this installation. Without a token it is safe only while the manager listens on localhost.
- The manager listens on `127.0.0.1:8080`. For LAN access set `BIND_ADDRESS=0.0.0.0` in `.env`, allow the port through the Windows firewall and forward it into WSL as described in the installation guide.

## Docker in other WSL distributions

WSL 2 runs every distribution in one network namespace. Docker Engines in different distributions keep their own images and containers, but they write their NAT, port and isolation rules into the same firewall, and none of them notices when another distribution removes its rules:

- An enabled `nftables` service removes every rule twice: the default `/etc/nftables.conf` it loads at boot starts with `flush ruleset`, and stopping it at shutdown runs `nft flush ruleset`. Desktops stop reaching the internet even with internet access on, and creating a desktop fails with `iptables: No chain/target/match by that name`. Installing Docker Engine on Ubuntu 24.04 brings in that package with the service enabled.
- A Docker Engine starting in another distribution rebuilds Docker's shared chains with only its own networks. Internet access keeps working, but the isolation between networks does not.

The package masks the `nftables` service, and `bilskirnir doctor` reports it when an installation from an older package still has it enabled. Mask it in the other distributions that run Docker too:

```bash
sudo systemctl mask nftables
```

The `bilskirnir-firewall` service repairs the damage when the rules disappear anyway. Every 10 seconds it checks that each Docker network still has its rule in Docker's `DOCKER-FORWARD` chain; when one is missing twice in a row, it restarts Docker. Docker runs with `live-restore`, so desktops and the manager keep running, but an operation talking to Docker at that moment, such as a script whose output is being streamed, can fail. `journalctl -u bilskirnir-firewall` lists the repairs and `bilskirnir doctor` shows the current state.

## Build the package

The package is produced from `wsl/Dockerfile` with Docker:

```bash
scripts/build-wsl.sh dist/bilskirnir.wsl
```

The script builds the root filesystem image, exports it with `docker export` and compresses it with `xz -9e`, which makes the package about 40% smaller than gzip; `WSL_COMPRESS=gzip` builds faster but produces a larger file. The package installs the manager version in `Cargo.toml`; `BILSKIRNIR_VERSION=1.2.0` builds one for another published release, and `BILSKIRNIR_VERSION=latest` one that follows `master`. It refuses to package a world-writable file, which a `COPY` without `--chmod` produces from a checkout on a Windows drive, or a directory its owner cannot enter, which `COPY --chmod=0644` produces when it creates the destination directory. Release tags publish the package as a GitHub release asset once the tag's images are published. The files under `wsl/` define the distribution: `wsl.conf` enables systemd, `wsl-distribution.conf` names the distribution and points WSL at the first-start script, `oobe.sh` performs that setup, `bilskirnir` is the management command, `keepalive.sh` is the process the Windows launcher keeps open, `daemon.json` configures Docker and `firewall-watch.sh` with `bilskirnir-firewall.service` restores Docker's firewall rules. The image masks `systemd-firstboot`, which would otherwise wait on the console for a machine ID and block every other unit, `kmod-static-nodes`, which has no module tree in WSL, and `nftables`, which flushes the firewall shared by all distributions; it also installs the systemd user-session packages so launches do not warn about a missing user session.

## Remove

`.\windows\bilskirnir.ps1 uninstall` removes the auto-start task and, after confirmation, unregisters the distribution. Unregistering deletes its Docker images, desktops and named volumes, so export anything you want to keep first.
