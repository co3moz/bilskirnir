# Using Bilskirnir

[Back to the README](../README.md)

An environment is an Ubuntu desktop running in a Docker container. An image is the starting point for that desktop. A volume holds files separately from the container, so you can keep using them after the environment is gone.

## Find your way around

The sidebar leads to **Dashboard**, **Environments**, **Images**, **Volumes** and **Jobs**, lists the desktops that are running for a quick connect and holds **New environment** with a hammer button for **Build image**. On a phone it opens from the button at the top left. The menu at the bottom of the sidebar switches between the dark, light and system theme and signs out.

The **Dashboard** sums up the host: how many environments exist and run, jobs in progress and today's failures, and the processors and memory Docker reports with the limits running desktops use. Below it, a chart shows finished and failed jobs per day, and a table lists every environment with tabs for running, stopped and those that need attention. Select several rows to start or stop them together. Select a name to open its details panel. **Environments** shows the same desktops as cards with a live preview.

## Create and connect

Select **New environment** and choose a desktop profile, custom image or snapshot. Give it a name and choose the resolution. You can also set CPU and memory limits, GPU access, internet access, sudo, sound, microphone, a virtual gamepad device for agents and volume mounts.

After creation finishes, select **Connect**. The card preview is a screenshot; the connected desktop accepts keyboard and mouse input. Use the environment's actions menu or its details panel to stop, restart, change settings, inspect or delete it. A desktop's address, such as `#/desktop/<id>`, can be bookmarked.

The GPU option is available only when the installation enables `WSL_GPU` or `NVIDIA_COMPUTE`. There is no fixed environment quota. Leaving CPU and memory limits empty uses Docker's unlimited defaults, so choose limits according to the machine and applications you are running. In the WSL package, all desktops share the memory WSL gives its virtual machine; [Memory, processors and disk](WSL_PACKAGE.md#memory-processors-and-disk) explains how to raise it. Enabling GPU access does not reserve GPU memory or set a VRAM limit.

## Choose XFCE or GNOME

Both profiles are Ubuntu 24.04. XFCE uses fewer resources and is a good starting point for application work. GNOME provides the Ubuntu shell and Yaru theme when you prefer that desktop.

The GNOME profile runs an X11 session in the container. It does not include GDM, a Wayland login session or host power and USB management. Screen locking is disabled; access goes through the manager's login.

A profile's image is downloaded the first time you create a desktop or build an image from it. The job panel shows the download's size and progress, and later desktops from that profile start without it. Under **Images**, **Download** on a profile card fetches it ahead of time and **Update** fetches its latest published release. `DESKTOP_IMAGE` sets the default selection. Custom builds and snapshots keep their source desktop's profile information.

## Running graphics applications

The desktop uses TigerVNC/Xvnc for its display. GPU applications use VirtualGL with Mesa D3D12 to reach the WSL GPU. In a GPU-enabled environment, use the supplied `gpu-run` wrapper:

```bash
gpu-run glxinfo -B
gpu-run /path/to/Unity -force-glcore
gpu-run /path/to/MyLinuxGame -force-glcore
```

The Unity and game commands are launch examples, not confirmation that a particular application version works. Install your Linux application first and test its graphics requirements. Windows executables and Wine are not included.

Chrome, Edge and other Chromium-based browsers, and Electron applications, keep their own sandbox. Desktops can create the user namespaces those sandboxes are built on, so these applications start from the menu, a terminal or a link another application opens in the default web browser, and `about:sandbox` reports the namespace sandbox. An installation with `DESKTOP_USER_NAMESPACES=false` keeps Docker's default seccomp profile instead: these applications then exit at startup unless they get `--no-sandbox`, and a link opened in such a default browser fails with "Failed to execute default Web Browser". A desktop created before an upgrade keeps the profile it was created with until a reset or a settings change recreates it.

Chromium-based browsers need `gpu-run` plus flags of their own, because their GPU process picks its X visual and its OpenGL path without VirtualGL's help:

```bash
gpu-run google-chrome --use-gl=angle --use-angle=gl --ignore-gpu-blocklist
```

For command names of the Chromium family (`google-chrome`, `chromium`, `brave`, `microsoft-edge`, `vivaldi`, `opera`, `electron`), `gpu-run` sets `VGL_CHROMEHACK=1`, VirtualGL's documented workaround for the browser's visual selection; `about:gpu` then reports hardware acceleration for WebGL, WebGPU, canvas, compositing and video decode through `ANGLE (D3D12 (NVIDIA ...))`. Video encode stays in software, and Vulkan stays disabled because the image has no Vulkan driver for the WSL GPU. Chrome runs its GPU process outside the sandbox by itself, so the WSL GPU device stays reachable while pages stay sandboxed. Use a dedicated profile with `--user-data-dir` for testing.

The workaround is not applied to other programs because it attaches the browser's 32-bit visual to every OpenGL configuration with an alpha channel, which would change the window visual of a game or Unity player that requests one. An Electron application with another name, or any program that needs it, gets it with `VGL_CHROMEHACK=1 gpu-run <command>`; `VGL_CHROMEHACK=0` turns it off for a browser.

GPU-enabled startup checks the renderer and rejects software rendering. If a GPU desktop fails to start, open **Logs**: the renderer check is written to the container log before the desktop exits, and **Diagnostics** needs a running desktop. Use **Diagnostics** when the desktop runs but rendering does not work. Hardware OpenGL has been verified on NVIDIA RTX 4080; Unity itself, Vulkan and AMD/Intel graphics have not been verified.

`WSL_GPU` enables WSL graphics support at installation level. `NVIDIA_COMPUTE` separately requests NVIDIA CUDA access. Turning CUDA off leaves WSL graphics available when `WSL_GPU=true`. `GPU_ADAPTER` selects a Mesa adapter by name, such as `NVIDIA`, `AMD`, `Intel` or part of its model name. Each environment still has its own GPU setting.

## Make an image with a script

Under **Images**, select **Build image**, choose a desktop base and enter a shell script. The script runs as root during the build, which lets you install packages and applications. Enable build internet access if your script needs to download them. The job log shows the script's output and the resulting image.

For example, this installs a couple of tools on either supplied Ubuntu profile:

```bash
set -eu
apt-get update
apt-get install -y --no-install-recommends git curl
rm -rf /var/lib/apt/lists/*
```

Keep the desktop startup, VNC and GPU components in place when adding software. The result must still start as a Bilskirnir desktop.

To reuse a build, select **Export configuration**, the copy button on its row under **Images**. It copies the image's name, base image, internet setting and script to the clipboard as JSON. In **Build image**, **Import configuration** takes that JSON and fills in the form, so you can change the name or base image before building, here or on another installation. A base image that is not available on this host is pointed out instead of selected. Snapshots and imported images have no script, so they have no configuration to export, and builds from an earlier release did not record theirs. [An archive](#move-an-image-to-another-installation) moves any of them.

**Import image** with its **Docker image** tab registers a compatible image that is already present in the host Docker Engine. It does not turn an arbitrary server image into a desktop. An image removed from Docker cannot be used to create another environment until it is available again.

## Save a desktop as a snapshot

Use **Snapshot** after installing applications or changing settings in an environment. Bilskirnir briefly pauses the container and uses Docker commit to save its filesystem changes as a new image layer.

For example, take a snapshot after installing an editor, then create a new environment from it to try a plugin. The new environment starts with the editor already installed. Changes made in the new environment do not change the saved image or the original desktop.

The image tree follows Docker ancestry. A snapshot is based on the image that originally created its container. Taking two snapshots of that same container produces siblings. To extend one of those snapshots, create a new environment from it and snapshot the new environment.

| Saved in the snapshot | Kept separately or not saved |
| --- | --- |
| Installed packages and application files in the container | Files in mounted volumes |
| Desktop settings and home files, unless mounted | Temporary files under `/run` and `/tmp` |
| Changes in the container's writable layer | Running processes, RAM and GPU state |

An image that declares a Docker `VOLUME` needs an explicit named-volume mount for that path when you create an environment. This makes it clear which files will be excluded from snapshots.

## Ask for values when a desktop is created

A template makes an image ask for a few values each time a desktop is created from it, such as the account to sign in with, and runs a short setup script with them before the desktop is yours. Under **Images**, select **Edit template**, the template button on the row of a snapshot, script build or imported image. Add a parameter for each value:

- **Name** is the environment variable the script reads, such as `UNITY_EMAIL`.
- **Label** is what **New environment** asks for.
- **Secret** makes it a password field. Its value is masked in job logs, never shown again and cannot have a default.
- **Required** values must be filled in, and a **Default** fills in the field.

For example, an image with Unity Hub and an editor installed could ask for a Unity account and a serial number, and activate the license with them:

```bash
set -eu
editor=$(ls -d ~/Unity/Hub/Editor/*/Editor/Unity | head -n 1)
"$editor" -batchmode -quit -nographics -logFile - \
    -username "$UNITY_EMAIL" -password "$UNITY_PASSWORD" -serial "$UNITY_SERIAL"
```

with the parameters `UNITY_EMAIL` (required), `UNITY_PASSWORD` (required, secret) and `UNITY_SERIAL` (secret). The script runs as the desktop user unless **Run as root** is on.

When you choose such an image in **New environment**, its fields appear below the image. The creation job waits until the desktop is ready, runs the script and finishes when the script does; its log shows the script's output. If the script fails, the desktop keeps running so you can look at it, and **Retry setup** in its details runs the script again. A reset runs it again too, since the reset desktop starts from the image's files.

The desktop keeps the template it was created with, so editing the image's template later affects only new desktops. A snapshot of the desktop keeps that template: a desktop created from the snapshot asks for the values again, so write the script so it can run where it already ran. Whatever the script writes, such as a license file, is part of the desktop's files and therefore of its snapshots.

**Build image** has a **Template** tab that gives the new image a template, and **Export configuration** and image archives carry it along.

## Move an image to another installation

Under **Images**, the archive button on an image's row (**Export archive**) downloads it as a `.tar.gz` file, with its name, description and desktop profile inside. A script build's configuration travels along, so **Export configuration** still works after the move. Snapshots, script builds and imported images can all be exported. The archive holds every layer of the image, the base desktop included, so expect several gigabytes. Docker prepares the whole image before the download starts to grow; **Jobs** shows the export and any error.

On the other installation, select **Import image** and drop the file on the **Archive file** tab. The name and description from the archive fill in, and you can change them. The import is checked while it uploads: it must hold one Bilskirnir desktop image and stay within `IMAGE_ARCHIVE_MAX_MB` (64 GiB by default) once unpacked. Tags and image names inside the archive are ignored, so an archive cannot replace a profile or another image. The image appears with the kind **archive** once Docker has loaded it.

A `.tar` saved with `docker save` imports the same way, as long as it holds a desktop image. Volumes are never part of an archive; copy their files separately.

## Keep files in volumes

Create a named volume under **Volumes**, then mount it when creating an environment. Use an absolute path such as `/workspace/project` or `/opt/game`. Protected system paths and read-only mounts over the desktop's home or configuration are rejected.

The desktop user runs as UID/GID **1001:1001**. Existing volumes need permissions that allow this user to work with their files.

Mounting the same writable volume in two environments gives both access to the same files. Taking a snapshot does not copy that volume. Deleting an environment preserves named volumes and saved images; unsaved changes left only in the container are lost.

## Upload files from the browser

Drop files onto a running environment card, the connected desktop or the upload dialog. You can also select **Upload files** to use the file picker.

Choose **Downloads** or a writable mounted volume as the destination. You can enter a subfolder and upload multiple files. Each file shows progress and a final status. Failed or cancelled files can be retried. Cancelling the queue keeps files that have already finished.

Existing files are preserved by default. Select **Replace existing files** when you intend to overwrite them. Uploads run as the desktop user, so a volume must be writable by UID/GID 1001:1001.

Downloads is part of the container filesystem unless you mount it. Use a named volume for files that should survive environment deletion. The default limit is 1 GiB per file, configurable with `UPLOAD_MAX_MB`.

You can also use Docker's file-copy commands from WSL. Replace `ENVIRONMENT_ID` with your environment's ID:

```bash
docker cp ./project wsld-ENVIRONMENT_ID:/workspace/project
docker cp wsld-ENVIRONMENT_ID:/home/desktop/Downloads/result ./result
```

## Change an environment's settings

Open an environment's actions menu and select **Settings** to change its name, resolution, limits, GPU, internet, sudo, sound, microphone, gamepad or volumes. The image cannot change; take a snapshot or reset the desktop instead.

The name, internet access and a new CPU or memory limit apply while the desktop keeps running. Docker fixes the other settings, and removing a limit, when it creates a container, so changing them creates a new one: Bilskirnir stops the desktop, saves its files to an image, starts a new container from it with the new settings and removes the old one. The dialog says so before applying. Open applications close, but installed software and files in the container are kept, as are named volumes. If the new container cannot start, the previous one is restored.

## Internet, sound and microphone

The internet switch attaches or detaches the environment's Docker egress network. Its private connection to the manager stays in place, so you can keep using the desktop while internet access is off. This is a Docker network setting, not a general firewall policy for the host or LAN.

Sound and microphone must be enabled both for the installation and for the environment. Use the browser toolbar controls to start each channel. Microphone input also needs browser permission. Localhost supports this over HTTP; access from another device requires HTTPS. Leaving the desktop closes the media connections and microphone tracks.

## Read job results

Longer operations appear under **Jobs**. Open a job to see its status and output; the panel updates while work continues. Image builds show script output. Image downloads show a progress bar with the downloaded size, and the job list shows their percentage. Deletion jobs explain which container and networks were removed and confirm that named volumes and saved images were preserved.

If environment creation failed partway through, **Retry setup** can complete missing setup without replacing an existing container. Agents can also reset an environment through the API, which recreates the container from its saved image while keeping named volumes. See [recovery](INSTALLATION.md#when-setup-is-interrupted) for what can and cannot be restored.
