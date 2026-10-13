# Bilskirnir

![Bilskirnir dashboard with Ubuntu desktop previews](docs/screenshots/bilskirnir.png)

Bilskirnir lets you create Ubuntu desktops in Docker and use them from your browser. It runs on WSL2, with GPU access for Linux graphics applications and a choice of XFCE or GNOME.

Use it when you want to install an application, try a different setup or keep several work environments on one machine. Prepare a desktop once, save it as an image, then start another environment from that image whenever you need it. Your working files can live in Docker volumes so they stay available even after you delete the desktop.

The name is a nod to Thor.

## What you can do

- Create separate desktops with their own applications, resolution and CPU/memory settings.
- Open a desktop in the browser, with optional sound and microphone support.
- Install software with a build script or directly in a desktop, then save the result as an image.
- Start new desktops from saved images and see how snapshots relate to their parents.
- Turn an environment's internet connection on or off without closing its desktop.
- Drag files onto a desktop or environment card to upload them to Downloads or a mounted volume.
- Let an agent manage desktops through the built-in MCP server, including files, command execution, background applications, window control and mouse/keyboard input with screenshots.

Bilskirnir currently serves one administrator on a local machine or LAN. The desktops are containers sharing the WSL kernel. The manager controls them through the host Docker socket; it does not run a nested Docker daemon.

## Getting started

Run the commands below **inside x86_64 WSL2 Ubuntu**, with Docker Engine and Compose v2 already installed.

The example enables NVIDIA graphics and CUDA. WSL GPU access must be working first: `/dev/dxg` and `/usr/lib/wsl` must exist, and CUDA requires NVIDIA Container Toolkit configured for your Docker Engine. For AMD/Intel, set `NVIDIA_COMPUTE` to `"false"` and change `GPU_ADAPTER`. To start without a GPU, set both `WSL_GPU` and `NVIDIA_COMPUTE` to `"false"`; the GPU option is then unavailable when creating a desktop.

Save this as `compose.yml` and change `ADMIN_PASSWORD` to a password of at least eight characters. Everything else can stay as shown for the NVIDIA setup:

```yaml
name: bilskirnir

services:
  manager:
    image: ghcr.io/co3moz/bilskirnir:latest
    container_name: bilskirnir-manager
    restart: unless-stopped
    ports:
      - "127.0.0.1:8080:8080" # Use 0.0.0.0 instead for LAN access.
    labels:
      io.wsl-desktop.manager: "true"
    environment:
      MANAGER_CONTAINER: bilskirnir-manager
      AUTH_MODE: password # Use none to disable login.
      ADMIN_USERNAME: admin
      ADMIN_PASSWORD: "replace-with-your-password"
      DESKTOP_IMAGE: ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04
      # These settings apply GPU access to desktops created in the UI.
      WSL_GPU: "true"
      NVIDIA_COMPUTE: "true" # Set false for AMD/Intel or without CUDA support.
      GPU_ADAPTER: NVIDIA # Use AMD / Intel for other adapters.
      AUDIO_ENABLED: "true"
      MICROPHONE_ENABLED: "true"
      UPLOAD_MAX_MB: "1024"
      ALLOWED_HOSTS: localhost
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
      - manager-data:/data
    depends_on:
      desktop:
        condition: service_completed_successfully
      desktop-gnome:
        condition: service_completed_successfully

  # Prepare both desktop images automatically; these services exit normally.
  desktop:
    image: ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04
    entrypoint: ["/bin/true"]
    restart: "no"
    network_mode: none
    healthcheck:
      disable: true

  desktop-gnome:
    image: ghcr.io/co3moz/bilskirnir/ubuntu-gnome:24.04
    entrypoint: ["/bin/true"]
    restart: "no"
    network_mode: none
    healthcheck:
      disable: true

volumes:
  manager-data:
    name: bilskirnir-data
```

Start Bilskirnir:

```bash
docker compose up -d
```

The first start downloads the manager and both desktop images. You do not need to clone this repository or build anything locally.

Open [localhost:8080](http://localhost:8080) and sign in as `admin` using your chosen password. Select **New environment**, choose XFCE or GNOME, then create it. Once it is running, select **Connect** to open the desktop.

The two short-lived `desktop` services in Compose only prepare the images. They exit successfully; you create your actual desktops in the web interface.

## Your first reusable desktop

Suppose you want a desktop with an editor and a few development tools. Start with XFCE or GNOME, install what you need, and take a **Snapshot**. You can now create another desktop from that snapshot without repeating the installation.

Keep project files in a named volume if you want them to outlive a desktop. Snapshots save changes in the container filesystem, including home files unless home is mounted. They do not include mounted volumes or the state of running applications.

For repeatable installation, use **Build image** with a shell script instead. Both approaches produce images you can use for new environments.

## Desktop and GPU support

Both supplied desktops use Ubuntu 24.04:

| Desktop | When to choose it |
| --- | --- |
| Lightweight XFCE | A smaller desktop for everyday tools and applications. |
| GNOME | Ubuntu's GNOME Shell session and Yaru theme, with higher resource use. |

Each has its own virtual display and VNC server. The manager shows a screenshot on the dashboard and opens an interactive noVNC session when you connect. No monitor is required.

Hardware OpenGL has been verified with VirtualGL and Mesa D3D12 on WSL Ubuntu 22.04, Docker Engine 28.1.1 and an NVIDIA RTX 4080. Unity Editor and Linux games are intended uses, but Unity itself has not yet been tested. Vulkan, other GPU vendors and physical browser microphone hardware also need verification. See the [GPU guide](docs/USAGE.md#running-graphics-applications) for launch commands.

## Where to go next

- [Using Bilskirnir](docs/USAGE.md): desktops, custom images, snapshots, files and volumes.
- [Installation and maintenance](docs/INSTALLATION.md): settings, LAN access, HTTPS, updates and recovery.
- [Development](docs/DEVELOPMENT.md): source builds, project structure, checks and image publishing.
- [Agent access with MCP](docs/MCP.md): connection settings, tools and automation workflows.
- [HTTP API](docs/API.md): use the same operations from scripts or another application.
- [Interface design](docs/DESIGN.md): visual choices and guidance for interface changes.
- [Verification notes](docs/BUG_HUNT.md): the earlier bug audit and what was checked.

Bilskirnir is written in Rust with a plain JavaScript frontend and is released under the [MIT license](LICENSE). Its bundled components keep their [own licenses](THIRD_PARTY_NOTICES.md).
