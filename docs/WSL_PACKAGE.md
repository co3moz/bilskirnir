# Installing Bilskirnir as a WSL distribution

[Back to the README](../README.md)

The `bilskirnir.wsl` package is a ready-made WSL distribution: Ubuntu 24.04 with Docker Engine, Compose, Buildx, the NVIDIA Container Toolkit and the Bilskirnir installation files. Installing it replaces the "install Ubuntu, install Docker, configure the GPU, prepare Compose" steps with one command. The distribution runs its own Docker Engine, so it does not touch Docker in another WSL distribution.

The package contains no application images. The first start downloads the manager from GHCR, and each desktop profile downloads the first time a desktop is created from it. This keeps the package small, leaves profiles you never use off the disk and lets `bilskirnir update` fetch newer images later.

## Requirements

- Windows 11 or Windows 10 with WSL 2.4.4 or newer (`wsl --version`), updated with `wsl --update`.
- For GPU desktops, a Windows GPU driver with WSL support. The NVIDIA Windows driver provides CUDA inside WSL; no Linux display driver is installed in the package. See [NVIDIA's WSL guide](https://docs.nvidia.com/cuda/wsl-user-guide/index.html).
- About 2 GB of disk space for the distribution, plus the manager image, each desktop profile you use (XFCE about 1.2 GB, GNOME about 2.3 GB, sharing their base layers) and your desktops.

## Install

Download `bilskirnir.wsl` from a release, then either double-click it or run:

```powershell
wsl --install --from-file .\bilskirnir.wsl
```

Open the new **Bilskirnir** entry from the Start menu or run `wsl -d Bilskirnir`. The first start asks for one username and password. They create the Linux user and are also the dashboard sign-in; an empty password leaves the Linux user without one and turns dashboard sign-in off (`AUTH_MODE=none`). A password needs at least eight characters. The setup then detects the WSL GPU device and CUDA libraries, generates a random agent token, writes the settings to `/opt/bilskirnir/.env` and offers to start Bilskirnir. Starting downloads the manager and brings it up; the dashboard is then at [localhost:8080](http://localhost:8080). The setup ends by printing the agent endpoint, token and client settings, which `bilskirnir mcp` shows again later.

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
| `bilskirnir update` | Pulls the latest manager and the desktop profiles already downloaded, then recreates the manager. |
| `bilskirnir settings` | Opens `/opt/bilskirnir/.env`; run `bilskirnir restart` afterwards. |
| `bilskirnir doctor` | Checks systemd, Docker, the GPU device, CUDA libraries, settings and registry reachability. |
| `bilskirnir open` | Opens the dashboard in the Windows browser. |
| `bilskirnir mcp` | Shows the agent endpoint, its token and ready-to-paste client settings. |

The installation files live in `/opt/bilskirnir`: `compose.yml` (the published-image manifest), `.env` and `deploy/Caddyfile`. Everything in the [installation guide](INSTALLATION.md) applies, including LAN access and HTTPS; run the Compose commands from `/opt/bilskirnir`.

## What the package decides for you

- The Linux user created at first start has passwordless `sudo` and belongs to the `docker` group, and its name is the dashboard's `ADMIN_USERNAME`. The distribution is a single-purpose appliance for the Windows user who installed it.
- Without a password, the dashboard has no sign-in. That is safe only while it listens on localhost; set `AUTH_MODE=password` and an `ADMIN_PASSWORD` in `.env` before allowing LAN access.
- Docker runs with the NVIDIA runtime registered. `WSL_GPU` is set when `/dev/dxg` and the WSL libraries exist, and `NVIDIA_COMPUTE` when `libcuda` is present; otherwise desktops run without GPU acceleration. Change these in `.env` if the detection is wrong.
- Agent access is enabled: `MCP_ENABLED=true` with a random 64-character `MCP_TOKEN` generated at first start. `bilskirnir mcp` prints the token and the [client settings](MCP.md); the token grants administrator access to every desktop, so set `MCP_ENABLED=false` in `.env` and restart if no agent will use this installation.
- The manager listens on `127.0.0.1:8080`. For LAN access set `BIND_ADDRESS=0.0.0.0` in `.env`, allow the port through the Windows firewall and forward it into WSL as described in the installation guide.

## Build the package

The package is produced from `wsl/Dockerfile` with Docker:

```bash
scripts/build-wsl.sh dist/bilskirnir.wsl
```

The script builds the root filesystem image, exports it with `docker export` and compresses it with gzip (`WSL_COMPRESS=xz` for a smaller file). Release tags publish the package as a GitHub release asset. The files under `wsl/` define the distribution: `wsl.conf` enables systemd, `wsl-distribution.conf` names the distribution and points WSL at the first-start script, `oobe.sh` performs that setup, `bilskirnir` is the management command and `keepalive.sh` is the process the Windows launcher keeps open. The image masks `systemd-firstboot`, which would otherwise wait on the console for a machine ID and block every other unit, and `kmod-static-nodes`, which has no module tree in WSL; it also installs the systemd user-session packages so launches do not warn about a missing user session.

## Remove

`.\windows\bilskirnir.ps1 uninstall` removes the auto-start task and, after confirmation, unregisters the distribution. Unregistering deletes its Docker images, desktops and named volumes, so export anything you want to keep first.
