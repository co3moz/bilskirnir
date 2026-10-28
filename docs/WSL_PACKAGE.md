# Installing Thoric as a WSL distribution

[Back to the README](../README.md)

The `thoric.wsl` package is a ready-made WSL distribution: Ubuntu 24.04 with Docker Engine, Compose, Buildx, the NVIDIA Container Toolkit and the Thoric installation files. Installing it replaces the "install Ubuntu, install Docker, configure the GPU, prepare Compose" steps with one command. The distribution runs its own Docker Engine, with images and containers separate from Docker in any other WSL distribution; only the network is shared, see [Docker in other WSL distributions](#docker-in-other-wsl-distributions).

The package contains no application images. The first start downloads the manager from GHCR, and each catalog image downloads the first time a container is created from it. This keeps the package small, leaves images you never use off the disk and lets `thoric update` fetch newer images later.

Each package installs the manager release it belongs to. The package from release 1.2.0 writes `MANAGER_IMAGE=ghcr.io/co3moz/thoric:1.2.0` to `.env`, so installing it months later still gives you manager 1.2.0, however many releases came out since, and that manager downloads the catalog images published with 1.2.0. Only [`thoric update`](#update) moves it forward.

## Requirements

- Windows 11 or Windows 10 with WSL 2.4.4 or newer (`wsl --version`), updated with `wsl --update`.
- For GPU containers, a Windows GPU driver with WSL support. The NVIDIA Windows driver provides CUDA inside WSL; no Linux display driver is installed in the package. See [NVIDIA's WSL guide](https://docs.nvidia.com/cuda/wsl-user-guide/index.html).
- About 2 GB of disk space for the distribution, plus the manager image, each catalog image you use (XFCE about 1.2 GB, GNOME about 2.3 GB, sharing their base layers) and your containers.

## Install

The package installs in one of two ways. Both give the same distribution, and the Windows helper works with one installed either way.

### Option 1: the Windows helper

Thoric's PowerShell helper, `thoric.ps1`, is published beside the package in every release and runs straight from there. In PowerShell:

```powershell
irm https://github.com/co3moz/thoric/releases/latest/download/thoric.ps1 | iex
```

`irm` downloads the helper and `iex` runs it, so nothing is saved and no script policy is in the way. While Thoric is not installed, it asks for a folder for the distribution's disk, which holds every image, container and volume (Enter keeps it under `%LOCALAPPDATA%\wsl`), and for the disk's maximum size (Enter for 1 TB, see [Disk](#disk)). It then downloads `thoric.wsl` from the latest release, registers the **Thoric** distribution and runs its first start in the same window. Run the line again later for [everything else it does](#the-windows-helper).

### Option 2: the package by hand

WSL installs the package by itself as well. Download it, then double-click it or run the second line; `--location D:\WSL\Thoric` and `--vhd-size 200GB` choose the disk's folder and maximum size:

```powershell
curl.exe -fLo thoric.wsl https://github.com/co3moz/thoric/releases/latest/download/thoric.wsl
wsl --install --from-file .\thoric.wsl
```

Then open the new **Thoric** entry from the Start menu or run `wsl -d Thoric` for the first start.

### First start

The first start asks for one username and password. They create the Linux user and are also the dashboard sign-in; an empty password leaves the Linux user without one and turns dashboard sign-in off (`AUTH_MODE=none`). A password needs at least eight characters.

Next it asks whether to enable root agent access, the MCP endpoint `/mcp` that reaches every container; move between the answers with the up and down arrows and confirm with Enter. **Yes** is the default, and it sets `ROOT_MCP_ENABLED`. [MCP endpoints](MCP.md#mcp-endpoints-for-one-agent) for chosen containers can be made on the dashboard later whatever the answer, each with its own token. With root agent access on, it asks how agents authenticate:

- **Random token** (the default) generates a 64-character token.
- **Custom token** takes a token you type: 1 to 256 visible ASCII characters without spaces or single quotes. It is stored in single quotes in `.env`, so `$` and `#` stay literal. A short token is easy to guess, and whoever has it controls every container.
- **None** sets `MCP_AUTH_MODE=none`: agents connect without a token, to `/mcp` and to every MCP endpoint, and anything that can reach the dashboard address controls every container.

Then it asks who can reach the dashboard, and with it the agent endpoint, which shares its address:

- **This computer only** (the default) sets `BIND_ADDRESS=127.0.0.1`. Windows programs reach it at localhost and nothing else on the network does.
- **Other devices too** sets `BIND_ADDRESS=0.0.0.0`, so the manager listens on every address of the distribution. Windows still decides whether other devices get through to it; the setup ends with the step for the WSL networking mode in use, and [Access from other devices](#access-from-other-devices) describes them all.

Before this question the setup warns when there is no password or agents need no token, since any device that reaches the address then controls every container.

Next it asks for the dashboard's port, `PORT`: Enter keeps 8080, or type another from 1 to 65535. Windows programs open the dashboard at localhost on that port, so choose another one when a program on Windows already uses 8080. With **Other devices too** it then asks for names other devices may open the dashboard by, such as the computer's name, separated by commas; they go into `ALLOWED_HOSTS` beside `localhost`. IP addresses need no name, and an empty answer lists none.

The setup then detects the WSL GPU device and CUDA libraries, writes the settings to `/opt/thoric/.env` and asks whether to start Thoric now, with the same kind of menu and **Yes** as the default. Starting downloads the manager and brings it up; the dashboard is then at [localhost:8080](http://localhost:8080), or at the port you chose. With agent access on, the setup ends by printing the agent endpoint and client settings, which `thoric mcp` shows again later.

A first start with piped input, such as an automated installation, reads one line for each question in the order they are asked. A menu's line holds the answer's first word (`yes`, `no`, `random`, `custom`, `none`, `this`, `other`) or its first letter, the port's a number and the names' a list separated by commas; an empty line or the end of the input picks the default. An answer it cannot use ends the setup before anything is created. It is not asked whether to start, and leaves that to `thoric start`.

## The Windows helper

The helper handles what has to happen outside the distribution. Run it with the same line as for installing; with Thoric installed, it shows whether Thoric is running and where its disk is, then a numbered list:

| Entry | Command | What it does |
| --- | --- | --- |
| Start it and keep it running | `start` | Boots the distribution, keeps it running and starts the manager, see [Keep it running](#keep-it-running). |
| Stop it | `stop` | Stops the manager and shuts the distribution down. |
| Show the manager state | `status` | Runs `thoric status`. |
| Open the dashboard | `open` | Opens the dashboard in the browser. |
| Update it to the newest release | `update` | Runs [`thoric update`](#update). |
| Move its disk to another folder | `move <folder>` | Moves the disk, see [Disk](#disk). |
| Change its disk's maximum size | `resize <size>` | Changes the disk's maximum size, see [Disk](#disk). |
| Start it at every Windows sign-in | `autostart enable` | Registers the auto-start task; with the task registered, the entry removes it instead (`autostart disable`). |
| Let other devices reach the dashboard | `lan enable` | Sets up Windows for [access from other devices](#access-from-other-devices); once that is set up, the entry removes it instead (`lan disable`). `lan status` shows what is set up. |
| Remove it with its images, containers and volumes | `uninstall` | Removes the auto-start task and, after confirmation, the distribution and what `lan enable` set up, see [Remove](#remove). |

`irm | iex` cannot pass a command. To run one without the list, for example from a script, make the helper a script block and pass the command after it:

```powershell
& ([scriptblock]::Create((irm https://github.com/co3moz/thoric/releases/latest/download/thoric.ps1))) move D:\WSL\Thoric
```

`install` takes `-Location <folder>`, `-VhdSize <size>` and `-Package <file>`, a package you already have. Every command takes `-Distro <name>` for a distribution installed under another name and `-Port <port>` for a dashboard port other than the one in `.env`, and `help` lists them all. A saved copy takes the same commands, run as `powershell -ExecutionPolicy Bypass -File .\thoric.ps1 <command>` because Windows runs a saved script only when allowed to; in a clone of the repository it is `windows\thoric.ps1`.

## Keep it running

WSL stops a distribution shortly after its last Windows-started process ends, even though Docker runs as a systemd service inside it. Two ways keep Thoric available:

- The helper's `start` boots the distribution, keeps a hidden keep-alive process open and starts the manager.
- Its `autostart enable` registers a scheduled task that runs the keep-alive process at Windows sign-in. Docker and the manager start with the distribution because the manager container uses `restart: unless-stopped`.

The helper's `stop` stops the manager and shuts the distribution down.

## Access from other devices

The first start asks whether other devices may reach the dashboard and the agent endpoint, and records the answer as `BIND_ADDRESS` in `.env`: `127.0.0.1` for this computer only, `0.0.0.0` for other devices too. Change it later with `thoric settings`, then run `thoric restart`. `thoric status` and `thoric doctor` show which one is in effect, and `doctor` also names the WSL networking mode. The port the first start asks for next is `PORT`, which changes the same way.

Set a password before opening it. Without one the dashboard has no sign-in, and with `MCP_AUTH_MODE=none` agents need no token, so any device that reaches the address controls every container. Over plain HTTP the password and the token cross the network readable; on a network you do not trust, use the [HTTPS service](INSTALLATION.md#https-and-microphone-access-over-lan). Other devices open the Windows computer's address, such as `http://192.168.1.20:8080`. A name instead of an address, such as the computer's name, has to be listed in `ALLOWED_HOSTS`, which the first start asks for and the dashboard's Management page changes.

`BIND_ADDRESS=0.0.0.0` opens the port inside the distribution. Whether other devices get through Windows to it depends on the WSL networking mode, a Windows setting for every distribution together, which the package leaves alone. The [Windows helper](#the-windows-helper) sets up what the mode needs: choose **Let other devices reach the dashboard**, or give it the command directly:

```powershell
& ([scriptblock]::Create((irm https://github.com/co3moz/thoric/releases/latest/download/thoric.ps1))) lan enable
```

It refuses while `BIND_ADDRESS` is still `127.0.0.1`, warns when there is no password or agents need no token, and asks for administrator rights through UAC. It opens one port, the dashboard's `PORT`; for the [HTTPS service](INSTALLATION.md#https-and-microphone-access-over-lan), `lan enable -Port 8443` opens its `HTTPS_PORT` in place of that. The helper's `lan status` shows what is set up and the address other devices open, and `lan disable` removes all of it, as `uninstall` does.

- **Mirrored** (`networkingMode=mirrored` under `[wsl2]` in `%UserProfile%\.wslconfig`): the distribution shares the network addresses of Windows, so the Hyper-V firewall only has to let the port in, through a rule named `Thoric`. Switching to mirrored networking changes it for every distribution and takes effect after `wsl --shutdown`, which stops them all.
- **NAT**, the WSL default: Windows forwards only its own localhost into WSL, so `lan enable` adds a port proxy from the port on every Windows address to the distribution and a Windows firewall rule named `Thoric`. The distribution gets a new address whenever WSL restarts, and the helper's `start` and the auto-start task then point the port proxy at it. Moving the port proxy needs administrator rights, so on an administrator's account `lan enable` also registers the task `Thoric LAN` with them: it reads an address from `%LOCALAPPDATA%\Thoric\wsl-address`, where the helper leaves it without those rights, and moves the port proxy there if it is an IPv4 address. It runs nothing from the distribution, so nothing inside gains administrator rights through it. On another account, `start` asks through UAC each time the address has changed. After `wsl --shutdown`, run `start` again.

The same by hand, in PowerShell as administrator, for port 8080:

```powershell
# Mirrored networking
New-NetFirewallHyperVRule -Name Thoric -DisplayName Thoric -Direction Inbound -VMCreatorId '{40E0AC32-46A5-438A-A0B2-2B479E8F2E90}' -Protocol TCP -LocalPorts 8080
# NAT networking; run the first two lines again whenever WSL restarts
$address = (wsl -d Thoric --exec ip -4 -o addr show dev eth0) -replace '.*inet ([0-9.]+)/.*', '$1'
netsh interface portproxy add v4tov4 listenport=8080 listenaddress=0.0.0.0 connectport=8080 connectaddress=$address
New-NetFirewallRule -Name Thoric -DisplayName Thoric -Direction Inbound -Protocol TCP -LocalPort 8080 -Action Allow
```

[Microsoft's WSL networking guide](https://learn.microsoft.com/windows/wsl/networking) covers both modes.

## Everyday commands

Inside the distribution, `thoric` manages the installation:

| Command | What it does |
| --- | --- |
| `thoric start` | Starts Docker if needed, downloads the manager if it is missing and starts it. |
| `thoric status` | Shows the Compose services, the dashboard address and who can reach it. |
| `thoric logs` | Shows the manager's recent log. |
| `thoric update` | Moves the manager, its `compose.yml` and the `thoric` command to the newest release, pulls the catalog images already downloaded, then recreates the manager. The dashboard's **Update** runs it too, see [Update from the dashboard](#update-from-the-dashboard). |
| `thoric settings` | Opens `/opt/thoric/.env` through `sudoedit` in your editor (`VISUAL` or `EDITOR`, nano by default); run `thoric restart` afterwards. The dashboard changes some settings itself, see [Settings from the dashboard](#settings-from-the-dashboard). |
| `thoric doctor` | Checks systemd, Docker, its firewall rules, memory, processors, free disk space, the GPU device, CUDA libraries, who can reach the dashboard with the WSL networking mode, settings and registry reachability. |
| `thoric open` | Opens the dashboard in the Windows browser. |
| `thoric mcp` | Shows the agent endpoint, its token and ready-to-paste client settings. |

Tab completes the commands: type `thoric re` and press Tab for `restart`. The package installs `bash-completion` for it, so `docker` completes as well. Packages before 0.6.0 have neither; an update does not add them.

`thor` is a short name for the same command: `thor status` is `thoric status`, and Tab completes it too.

The installation files live in `/opt/thoric`: `compose.yml` (the published-image manifest), `.env`, `.env.example` and `deploy/Caddyfile`, beside the `update` folder of [Update from the dashboard](#update-from-the-dashboard) and [Settings from the dashboard](#settings-from-the-dashboard), and `.env.previous` after a settings change from the dashboard. [`thoric update`](#update) keeps all but `.env` in step with the manager. Everything in the [installation guide](INSTALLATION.md) applies, including LAN access and HTTPS; run the Compose commands from `/opt/thoric`.

## Update

`thoric update` looks up the newest release on GitHub, downloads its manager, writes it to `MANAGER_IMAGE` in `.env` and recreates the manager. It never moves to a release older than the installed one, and leaves `.env` as it was when the lookup or the download fails. In the same step it downloads the catalog images you already have again, in the version the new manager uses: a release manager takes the images published with it, such as `ubuntu-xfce:1.2.0`, and tags them with their usual catalog reference, `ubuntu-xfce:24.04`, which containers and images record.

A `MANAGER_IMAGE` set by hand to anything other than a release of `ghcr.io/co3moz/thoric`, such as `edge`, which follows `master` (see [Edge builds](#edge-builds)), `latest`, which names the newest release, or a digest, is only pulled again. To return to releases, set it to one with `thoric settings`, for example `ghcr.io/co3moz/thoric:1.2.0`, and run `thoric update`.

Before it recreates the manager, an update copies the installation files that manager bundles, `compose.yml`, `deploy/Caddyfile` and `.env.example`, into `/opt/thoric`. A setting added in a release reaches the manager only through the `environment:` list in that release's `compose.yml`, so with an older file, setting it in `.env` would do nothing. `.env` is never replaced; compare it with the new `.env.example` to find the settings a release added.

The package and every update record a checksum of each of these files in `/opt/thoric/.shipped.sha256`, so an update can tell whether you edited one:

- A file you have not edited is replaced.
- A file you edited stays as it is when the release brings the same version of it as before.
- A file you edited is saved as `<file>.bak`, for example `compose.yml.bak`, when the release brings a new version, and is then replaced; the update names the file it saved. Carry over the changes you still need. The manager starts with the new file right away, so a change such as a different port mapping is missing until you carry it over and run `thoric restart`. An earlier `.bak` is replaced only when you edit the file again.

To keep your own Compose changes out of the way of updates, put them in `/opt/thoric/compose.override.yml` rather than editing `compose.yml`. Compose merges that file into `compose.yml` on its own, and updates never touch it.

### The thoric command

The manager image also bundles the `thoric` command of its release, so fixes to `update`, `doctor` or `mcp` reach an installation without a new package. Before it copies the installation files, an update replaces `/usr/local/bin/thoric` with the command of the release it moves to, but only when all of these hold:

- `MANAGER_IMAGE` is a release of `ghcr.io/co3moz/thoric`, as the package sets it. A manager you chose by hand, such as `edge`, `latest`, a digest or a fork, already runs with access to Docker, but as a container that the next change of `MANAGER_IMAGE` replaces. A command it brought would stay after you switch back, and would keep running as you with passwordless `sudo`. With such a manager, the update says the command stays as it is.
- The release is newer than the one the command came with. The command never moves back, not even when you set an older manager by hand.
- The distribution has the package files the new command relies on. The package records the level of its own files, such as the firewall watcher, in `/usr/lib/wsl/package-level`, and every command names the level it needs. When a release needs a newer package, the update says so, keeps the command and updates everything else. Only a newer package brings that command, and installing one replaces the distribution together with its Docker data, see [Remove](#remove).
- The new command is complete: it names the level it needs and passes a syntax check.

The new command is written beside the old one and renamed over it, so an interrupted update leaves one of the two whole. The update then starts over with the new command, which copies the installation files by its release's rules and recreates the manager. Changes you made to the command itself are not kept. The rest of the distribution, such as Docker, the firewall watcher and the first-start setup, stays as the package installed it, apart from the units of [Update from the dashboard](#update-from-the-dashboard) and [Settings from the dashboard](#settings-from-the-dashboard).

The command of the 0.3.0 package and of earlier ones cannot update itself, and those packages set `MANAGER_IMAGE` to `latest`. Install the command of a newer release once by hand, replacing `1.2.0` with it:

```bash
docker run --rm --entrypoint cat ghcr.io/co3moz/thoric:1.2.0 /app/install/thoric > /tmp/thoric
```

```bash
sudo install -m 0755 /tmp/thoric /usr/local/bin/thoric
```

Then set `MANAGER_IMAGE` to that release, as described above, and run `thoric update`. From then on, updates bring the command.

### Update from the dashboard

The manager looks up the newest release on GitHub when it starts and every six hours, and at once when you select **Check now** on the dashboard's **Management** page. When one is newer than the manager, the sidebar and that page name it, link to its notes and offer **Update**. **Update** runs the same `thoric update` in the distribution, as root: it moves `MANAGER_IMAGE` in `.env` to that release, brings the release's installation files and command, recreates the manager and downloads the catalog images you already have again. Containers keep running. The job's log is the command's output. The dashboard reloads once the new manager answers, and asks you to sign in again, since sessions end with the old manager; the new manager follows the rest of the update in the same job. The dashboard waits for running jobs to finish first, since they would end with the manager.

The manager cannot reach the distribution from its container, so the two meet in `/opt/thoric/update`, which `compose.yml` mounts into the manager at `/update`. The manager leaves a `request` there, `thoric-update.path` notices it and starts `thoric-update.service`, which renames it to `started`, writes the command's output to `log` and how it ended to `exit`. The request only names the dashboard's job; what runs is always `/usr/local/bin/thoric update`. `systemctl status thoric-update.service` and `/opt/thoric/update/log` show the last update. The dashboard offers **Update** once that folder holds `watched`, which the package and an update write when they install the two units. On another installation the sidebar only names the release.

A package from before the units gets them with its first `thoric update` to a release that bundles them. Like the command, they come only from a release of `ghcr.io/co3moz/thoric`, since they run as root and would stay after a manager chosen by hand is gone, and an update enables them only when it installs or changes them, so a path unit you turned off stays off. To turn off both the lookup and the button, set `UPDATE_CHECK=false` with `thoric settings` and run `thoric restart`; the manager then makes no request to GitHub.

### Edge builds

`edge` is the newest build of `master`, published with every change before a release includes it. To follow it, set `MANAGER_IMAGE=ghcr.io/co3moz/thoric:edge` with `thoric settings` and run `thoric update`. From then on, each `thoric update`, and **Update** on the dashboard once a newer release is out, downloads the newest `edge` again:

- The installation files come from the edge manager, so a setting added on `master` reaches it.
- The catalog images you already have come from the same commit as the manager, as the desktop images published with it, and are tagged as their catalog reference.
- The `thoric` command and the units of the dashboard's update and settings stay as the package or the last release brought them, since they run as root (see [The thoric command](#the-thoric-command)).
- The sidebar names the commit the manager was built from instead of a version, since an edge build still carries the version of the release before it.

To return to releases, wait for a release newer than the edge build you run, then set `MANAGER_IMAGE` to it, such as `ghcr.io/co3moz/thoric:1.2.0`, and run `thoric update`; updates then follow releases again. An older release is a step back that its version does not show: it may not read what the newer manager wrote, and it refuses the desktop images, and the images built on them, made for a newer Thoric.

## Settings from the dashboard

The dashboard's **Management** page changes some settings itself: the cookie and host names, the default image, the user namespaces, the GPU switches, the container features' switches and defaults, the upload and archive limits, the templates list, the volume filter and the release checks; [the installation guide](INSTALLATION.md#settings) names them. Each gets a control, and **Apply and restart** lists the changes and warns when running jobs would end with the manager. The rest, the sign-in, the root MCP and its token, `MANAGER_CONTAINER` and what only Compose reads, such as `PORT` and `BIND_ADDRESS`, are marked **CLI only**: change them with `thoric settings`, then run `thoric restart`.

`.env` stays the only place settings are kept. The manager checks each change with the code it reads its settings with when it starts, so a value it would refuse is refused before anything restarts, and leaves the change as `settings-request` in `/opt/thoric/update`, the folder of [Update from the dashboard](#update-from-the-dashboard). `thoric-settings.path` notices it and starts `thoric-settings.service`, which renames it to `settings-started` and runs `thoric apply-settings` with it as root, writing the output to `settings-log` and how it ended to `settings-exit`. The command:

- accepts only the settings the page may change, each value one line of printable ASCII without quotes, `$`, `#`, backslashes, backquotes or spaces at either end, and changes nothing when a line is anything else;
- waits for nothing and changes nothing while `thoric-update.service` runs;
- keeps the previous file as `/opt/thoric/.env.previous`, replaces only the lines of the changed settings, keeping comments, order and every other line, and adds a setting `.env` lacks at the end;
- recreates the manager and waits up to 90 seconds for it to answer its health check. When it does not, the command puts `.env.previous` back, recreates the manager again and fails, so the dashboard's job fails with the log saying why.

The job's log is the command's output. Desktops keep running; the page reloads once the new manager answers, and asks you to sign in again, since sessions end with the old manager, which then follows the rest of the job. `systemctl status thoric-settings.service` and `/opt/thoric/update/settings-log` show the last change. The page offers the controls once the folder holds `settings-watched`, which the package and an update write when they install the two units; like the update's units, they come only from a release, and a package from before them gets them with its first `thoric update`.

## Memory, processors and disk

Every WSL distribution runs in one shared virtual machine. Its memory and processor count are Windows settings for all distributions together, so the package cannot set them. The disk is different: each distribution has its own virtual disk, sized separately.

### Memory and processors

Unless configured otherwise, WSL gives the virtual machine half of the Windows memory and all of its processors, and Thoric shares them with every other running distribution. Change them in `%UserProfile%\.wslconfig` or in the **WSL Settings** app:

```ini
[wsl2]
memory=24GB
processors=12
swap=8GB
```

WSL reads these settings only when the virtual machine starts, and the helper's `stop` stops only this distribution. Apply them with `wsl --shutdown`, which stops every WSL distribution, then start Thoric again with the helper's `start`:

```powershell
wsl --shutdown
```

The manager comes back on its own; containers do not, so start them again from the dashboard. Their running applications and memory are lost, as with any stop. Afterwards the dashboard's **MEMORY** figure and `thoric doctor` show the new amount, slightly below the configured value because the kernel keeps part of it. `thoric doctor` warns when it is below 8 GB.

A container without a memory limit can use all of the virtual machine's memory. When that runs out, Linux ends processes inside the virtual machine, which can include the manager or another container; Windows itself stays protected by the limit above. Give containers that run heavy applications a memory limit when you [create them](USAGE.md#create-and-connect) or change their settings. A limit is a ceiling, not a reservation: memory a container does not use stays available to the rest.

### Disk

The distribution's virtual disk grows as it fills, up to a maximum of 1 TB unless chosen otherwise. Images, snapshots, containers and volumes all live in it, so the Windows drive that holds the disk needs the space too. Choose the folder and the maximum when installing: the helper asks for both, and a [package installed by hand](#option-2-the-package-by-hand) takes `--location` and `--vhd-size`.

To change them later, choose **Move its disk to another folder** or **Change its disk's maximum size** in the [helper](#the-windows-helper), or give it the command directly:

```powershell
& ([scriptblock]::Create((irm https://github.com/co3moz/thoric/releases/latest/download/thoric.ps1))) move D:\WSL\Thoric
& ([scriptblock]::Create((irm https://github.com/co3moz/thoric/releases/latest/download/thoric.ps1))) resize 200GB
```

WSL changes a disk only while nothing has it attached, and it keeps a stopped distribution's disk attached until the WSL virtual machine shuts down. So both stop the manager and run `wsl --shutdown`, which stops every WSL distribution; when others are running, they name them and ask first. Then they change the disk and start Thoric again. The manager comes back on its own; containers do not, so start them again from the dashboard. A move to another drive copies the whole disk, which takes a while for a large one, and the folder must not hold a disk already.

A smaller maximum works only while the data still fits. `thoric doctor` shows the space left before the maximum and warns below 10 GB.

The disk file does not shrink when images or containers are deleted. Two ways return freed space to Windows, and they exclude each other: run `wsl --manage Thoric --set-sparse true` once, which WSL still labels experimental, or run `wsl --manage Thoric --compact` from time to time. Both need the disk released as above: run the helper's `stop` and `wsl --shutdown` first and its `start` afterwards. A sparse disk cannot be compacted.

## What the package decides for you

- The Linux user created at first start has passwordless `sudo` and belongs to the `docker` group, and its name is the dashboard's `ADMIN_USERNAME`. The distribution is a single-purpose appliance for the Windows user who installed it.
- `.env` holds the dashboard password and the agent token, so it belongs to root and only the `docker` group may read it (`root:docker`, mode `0640`). `thoric settings` saves it through `sudoedit`, which keeps that owner and mode, as do a settings change from the dashboard and its `.env.previous`; change it any other way with `sudo`.
- Without a password, the dashboard has no sign-in. That is safe only while it listens on localhost; set `AUTH_MODE=password` and an `ADMIN_PASSWORD` in `.env` before allowing other devices.
- Docker runs with the NVIDIA runtime registered. `WSL_GPU` is set when `/dev/dxg` and the WSL libraries exist, and `NVIDIA_COMPUTE` when `libcuda` is present; otherwise containers run without GPU acceleration. Change these in `.env` if the detection is wrong.
- Root agent access is enabled unless the first start turned it off: `ROOT_MCP_ENABLED=true` with a random 64-character `MCP_TOKEN` by default, a custom token or `MCP_AUTH_MODE=none`. `thoric mcp` prints the endpoint, the token and the [client settings](MCP.md); root agent access grants administrator access to every container, so set `ROOT_MCP_ENABLED=false` in `.env` and restart if no agent needs that, and give agents [MCP endpoints](MCP.md#mcp-endpoints-for-one-agent) for chosen containers instead, which answer either way. Without a token it is safe only while the manager listens on localhost. Installations from before 0.6.3 have the setting as `MCP_ENABLED`, which keeps working until `thoric update` renames it.
- The manager listens on `127.0.0.1`, or on `0.0.0.0` when the first start allowed other devices, at the port the first start asked for, `8080` by default. Windows has to let them in as well, see [Access from other devices](#access-from-other-devices).

## Docker in other WSL distributions

WSL 2 runs every distribution in one network namespace. Docker Engines in different distributions keep their own images and containers, but they write their NAT, port and isolation rules into the same firewall, and none of them notices when another distribution removes its rules:

- An enabled `nftables` service removes every rule twice: the default `/etc/nftables.conf` it loads at boot starts with `flush ruleset`, and stopping it at shutdown runs `nft flush ruleset`. Containers stop reaching the internet even with internet access on, and creating a container fails with `iptables: No chain/target/match by that name`. Installing Docker Engine on Ubuntu 24.04 brings in that package with the service enabled.
- A Docker Engine starting in another distribution rebuilds Docker's shared chains with only its own networks. Internet access keeps working, but the isolation between networks does not.

The package masks the `nftables` service, and `thoric doctor` reports it when an installation from an older package still has it enabled. Mask it in the other distributions that run Docker too:

```bash
sudo systemctl mask nftables
```

The `thoric-firewall` service repairs the damage when the rules disappear anyway. Every 10 seconds it checks that each Docker network still has its rule in Docker's `DOCKER-FORWARD` chain; when one is missing twice in a row, it restarts Docker. Docker runs with `live-restore`, so containers and the manager keep running, but an operation talking to Docker at that moment, such as a script whose output is being streamed, can fail. `journalctl -u thoric-firewall` lists the repairs and `thoric doctor` shows the current state.

## Build the package

The package is produced from `packaging/wsl/Dockerfile` with Docker:

```bash
packaging/wsl/build.sh dist/thoric.wsl
```

The script builds the root filesystem image, exports it with `docker export` and compresses it with `xz -9e`, which makes the package about 40% smaller than gzip; `WSL_COMPRESS=gzip` builds faster but produces a larger file. The package installs the manager version in `core/manager/Cargo.toml`; `THORIC_VERSION=1.2.0` builds one for another published release, and `THORIC_VERSION=edge` one that follows `master`. It refuses to package a world-writable file, which a `COPY` without `--chmod` produces from a checkout on a Windows drive, or a directory its owner cannot enter, which `COPY --chmod=0644` produces when it creates the destination directory. Release tags publish the package and the Windows helper, `packaging/windows/thoric.ps1`, as GitHub release assets once the tag's images are published. The files under `packaging/wsl/` define the distribution: `wsl.conf` enables systemd, `wsl-distribution.conf` names the distribution and points WSL at the first-start script, `oobe.sh` performs that setup, `thoric` is the management command, `keepalive.sh` is the process the Windows launcher keeps open, `daemon.json` configures Docker and `firewall-watch.sh` with `thoric-firewall.service` restores Docker's firewall rules. The image masks `systemd-firstboot`, which would otherwise wait on the console for a machine ID and block every other unit, `kmod-static-nodes`, which has no module tree in WSL, and `nftables`, which flushes the firewall shared by all distributions; it also installs the systemd user-session packages so launches do not warn about a missing user session.

## Remove

The helper's **Remove it with its images, containers and volumes** (`uninstall`) removes the auto-start task and, after confirmation, unregisters the distribution and removes what `lan enable` set up on Windows. Unregistering deletes its Docker images, containers and named volumes, so export anything you want to keep first.
