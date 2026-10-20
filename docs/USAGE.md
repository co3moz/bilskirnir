# Using Bilskirnir

[Back to the README](../README.md)

A container is an Ubuntu desktop that Bilskirnir runs in Docker. An image is the starting point for a container. A compose belongs to an image and predefines what a new container from it asks for and sets up. A volume holds files separately from the container, so you can keep using them after the container is gone.

## Find your way around

The sidebar leads to **Dashboard**, **Containers**, **Images**, **Templates**, **Volumes**, **MCP**, **Files** and **Jobs**, lists the running containers for a quick connect and holds **New container** with a hammer button for **Build image**. On a phone it opens from the button at the top left. The menu at the bottom of the sidebar switches between the dark, light and system theme, chooses the language and signs out.

The dashboard speaks English, German, French, Spanish and Turkish. It follows the browser's language, or English when the browser prefers another one; a language picked in that menu, or on the sign-in page, is kept in this browser. Messages from the manager itself, such as job logs and errors, stay in English.

The **Dashboard** sums up the host: how many containers exist and run, jobs in progress and today's failures, and the processors and memory Docker reports with the limits running containers use. **Resource usage** follows each running container once a second: CPU in cores, memory, disk reads and writes and network traffic, one color per container and stacked, so the top edge is what they use together, for the last 5, 15 or 30 minutes. A table under the charts gives each container's current figures. A deleted container stays in the charts, marked as deleted and in its color, until its samples are 30 minutes old. While a GPU container runs, the host's NVIDIA GPU is charted beside them; that is the whole GPU, Windows applications included, since WSL does not report which container uses it. The history is kept in memory only and starts over when Bilskirnir restarts. A container's details panel shows its own charts against its CPU and memory limits. Below that, a chart shows finished and failed jobs per day, and a table lists every container with tabs for running, stopped and those that need attention. Select several rows to start or stop them together. Select a name to open its details panel. **Containers** shows the same containers as cards with a live preview.

## Create and connect

Select **New container**, give it a name and choose an image: XFCE or GNOME from the catalog, a script build, a snapshot or an imported image. Under **Features**, buttons turn GPU access, internet access, sudo, sound, microphone and a virtual gamepad device for agents on or off; hover or focus one to see what it does. **Advanced settings** holds the resolution, shared memory, CPU and memory limits, proxied ports and volume mounts; it starts closed with its current values in one line, and opens by itself when one of its fields needs a correction. **Proxied ports** lists ports of programs inside the desktop, separated by commas, that an agent holding the MCP token reaches through the manager, as [desktop ports](API.md#desktop-ports) describes; a change applies without a restart.

**Create an MCP endpoint**, on by default, gives the new container an endpoint of its own at `/mcp/<name>`, where an agent reaches only this container and the MCP servers running in it; see [MCP servers and endpoints](#mcp-servers-and-endpoints).

After creation finishes, select **Connect**. The card preview is a screenshot; the connected desktop accepts keyboard and mouse input. Use the container's actions menu or its details panel to stop, restart, change settings, inspect or delete it. A desktop's address, such as `#/desktop/<id>`, can be bookmarked.

The GPU option is available only when the installation enables `WSL_GPU` or `NVIDIA_COMPUTE`. There is no fixed container quota. Leaving CPU and memory limits empty uses Docker's unlimited defaults, so choose limits according to the machine and applications you are running. In the WSL package, all containers share the memory WSL gives its virtual machine; [Memory, processors and disk](WSL_PACKAGE.md#memory-processors-and-disk) explains how to raise it. Enabling GPU access does not reserve GPU memory or set a VRAM limit.

## Choose XFCE or GNOME

Both catalog images are Ubuntu 24.04. XFCE uses fewer resources and is a good starting point for application work. GNOME provides the Ubuntu shell and Yaru theme when you prefer that desktop.

The GNOME image runs an X11 session in the container. It does not include GDM, a Wayland login session or host power and USB management. Screen locking is disabled; access goes through the manager's login.

**Images** lists them with the source **Catalog**, above the images you save. A catalog image is downloaded the first time you create a container or build an image from it. The job panel shows the download's size and progress, and later containers from it start without it. On its row, the download button fetches it ahead of time and, once it is here, the update button fetches its latest published release. `DESKTOP_IMAGE` sets the default selection. Script builds and snapshots made from a catalog image appear under it and keep its desktop information.

## Running graphics applications

The desktop uses TigerVNC/Xvnc for its display. GPU applications use VirtualGL with Mesa D3D12 to reach the WSL GPU. In a GPU-enabled container, use the supplied `gpu-run` wrapper:

```bash
gpu-run glxinfo -B
gpu-run /path/to/Unity -force-glcore
gpu-run /path/to/MyLinuxGame -force-glcore
```

The Unity and game commands are launch examples, not confirmation that a particular application version works. Install your Linux application first and test its graphics requirements. Windows executables and Wine are not included.

Chrome, Edge and other Chromium-based browsers, and Electron applications, keep their own sandbox. Containers can create the user namespaces those sandboxes are built on, so these applications start from the menu, a terminal or a link another application opens in the default web browser, and `about:sandbox` reports the namespace sandbox. An installation with `DESKTOP_USER_NAMESPACES=false` keeps Docker's default seccomp profile instead: these applications then exit at startup unless they get `--no-sandbox`, and a link opened in such a default browser fails with "Failed to execute default Web Browser". A container created before an upgrade keeps the seccomp profile it was created with until a reset or a settings change recreates it.

In a GPU-enabled container these browsers and applications use the GPU however they are started: the web browser button in the panel, the applications menu, a link another application opens or a terminal, in XFCE and GNOME, with no flags and no `gpu-run`. `about:gpu` then reports hardware acceleration for WebGL, WebGPU, canvas, compositing and video decode through `ANGLE (D3D12 (NVIDIA ...))`. Video encode stays in software, and Vulkan stays disabled because the image has no Vulkan driver for the WSL GPU. Chrome runs its GPU process outside the sandbox by itself, so the WSL GPU device stays reachable while pages stay sandboxed. Without GPU access the same browsers start normally and render in software.

This needs `VGL_CHROMEHACK=1`, VirtualGL's documented workaround for the way Chromium's GPU process picks its X visual. The desktop session runs through `gpu-run`, which turns it on for each program whose executable sits beside Chromium's `icudtl.dat` and `resources.pak`, whatever the program is called and however it was installed. Other programs, including those a browser starts, keep VirtualGL's default: the workaround attaches the browser's 32-bit visual to every OpenGL configuration with an alpha channel, which would change the window visual of a game or Unity player that requests one. An explicit `VGL_CHROMEHACK` always wins, so `VGL_CHROMEHACK=1 gpu-run <command>` turns it on for any program and `VGL_CHROMEHACK=0` turns it off for a browser. A browser started outside the desktop session, such as from a script job or a terminal session of the API, needs `gpu-run` like any other GPU application.

GPU-enabled startup checks the renderer and rejects software rendering. If a GPU container fails to start, open **Logs**: the renderer check is written to the container log before the desktop exits, and **Diagnostics** needs a running desktop. Use **Diagnostics** when the desktop runs but rendering does not work. Hardware OpenGL has been verified on NVIDIA RTX 4080; Unity itself, Vulkan and AMD/Intel graphics have not been verified.

`WSL_GPU` enables WSL graphics support at installation level. `NVIDIA_COMPUTE` separately requests NVIDIA CUDA access. Turning CUDA off leaves WSL graphics available when `WSL_GPU=true`. `GPU_ADAPTER` selects a Mesa adapter by name, such as `NVIDIA`, `AMD`, `Intel` or part of its model name. Each container still has its own GPU setting.

## Make an image with a script

Under **Images**, select **Build image**, choose a desktop base and enter a shell script. The script runs as root during the build, which lets you install packages and applications. Enable build internet access if your script needs to download them. The job log shows the script's output and the resulting image.

For example, this installs a couple of tools on either catalog image:

```bash
set -eu
apt-get update
apt-get install -y --no-install-recommends git curl
rm -rf /var/lib/apt/lists/*
```

Keep the desktop startup, VNC and GPU components in place when adding software. The result must still start as a Bilskirnir desktop.

**Build image** also takes an optional description and an icon. Choose any picture, PNG, JPEG, WebP, GIF or SVG; it is scaled down to fit 256 by 256 pixels and saved as a PNG. **Images** shows the icon beside the name, and selecting it, or the plain image symbol of a saved image without one, changes or removes it. Snapshots keep the icon of the image their container came from.

To reuse a build, select **Export configuration**, the copy button on its row under **Images**. It copies the image's name, description, base image, internet setting, script and icon to the clipboard as JSON. In **Build image**, **Import configuration** takes that JSON and fills in the form, so you can change the name or base image before building, here or on another installation. A base image that is not available on this host is pointed out instead of selected. Snapshots and imported images have no script, so they have no configuration to export, and builds from an earlier release did not record theirs. [An archive](#move-an-image-to-another-installation) moves any of them.

## Install a template

**Templates** lists ready-made images from the [Bilskirnir wiki](https://github.com/co3moz/bilskirnir/wiki), each with its icon, a description and tags; search or pick a tag to narrow the list. **Applications** come first: desktop programs, most with an MCP server an agent drives them through. **Environments** follow below them: language toolchains such as Node.js, Python or Rust, which an agent builds and tests code with through Bilskirnir's terminal, command and file tools. An environment is meant to be merged with an application, such as Node.js with VS Code, with **Merge and install** below; it also installs on its own. Search, tags, **Merge and install** and **Installed** work across both sections. **Install** opens **Build image** with the template filled in, asking only for a name and a base image: the template's description, icon and MCP servers are shown as they are, and selecting **Build image** builds it with its own script and compose. **Advanced editing** opens the whole form, where you can read the script before building, since it runs as root, and change it, the description, the icon or anything else. The build then runs like any other, and the image appears under **Images**. A container created from it asks for the compose's values, such as an account, as usual.

To have several templates in one image, such as Unity development with Google Chrome, or VS Code with the Node.js environment, select **Merge and install**. The cards turn into choices: select each template you want, on its card or its checkbox, in the order its script should run; each chosen card shows its step number. **Merge** becomes available once two are chosen and opens **Build image** with all of them, asking again only for a name and a base image. Its **Advanced editing** shows every template's script in a section of its own, which you can read, change, run earlier or later, or leave out, and a compose with a section for each template that asks for values. Two templates may ask for the same value, such as a version each; a container created from the image asks for each template's values in a section of its own, and each script receives only its own. The image is built on the first template's base image, and the form warns when another template was made for a different one. **Cancel** leaves the choices.

A template you have built shows **Installed**, also when it was merged with others, and **Install again** builds another image from its current version. A template that needs a newer Bilskirnir says which release it needs instead of offering **Install**. Your browser reads the list directly from GitHub, so it needs internet access, and the page sends no request until you open it. An installation can point the page at another list or turn it off with `TEMPLATES_URL`; see [the installation guide](INSTALLATION.md#settings).

**Import image** with its **Docker image** tab registers a compatible image that is already present in the host Docker Engine. It does not turn an arbitrary server image into a desktop. An image removed from Docker cannot be used to create another container until it is available again.

## MCP servers and endpoints

A desktop can run MCP servers of its own, such as a browser automation server a template brings, or one you write while developing a Chrome extension. The **MCP** page shows them under **Desktop servers**, grouped by container: where each came from (**Image** for a template's, **Added** for one added later), its state, its tools and the end of its output. **Add server** registers one in a running container: a command that speaks MCP on stdio, or the address of an HTTP server listening inside the desktop. Inside the desktop, `bilskirnir-mcp add NAME -- COMMAND ARGS` does the same from a terminal. Bilskirnir starts a server as the desktop user only when its tools are needed and never opens a port for it. A value such as `${API_TOKEN}` in its arguments or headers is filled in from the container's compose values.

**Endpoints** lists the addresses agents connect to instead of the administrator's `/mcp`: each covers one or several containers, and for each only the groups of tools you tick, such as screenshots, mouse and keyboard or files, and its servers, all of them or the ones you pick. Each ticked group and each included server shows its tools behind their count; untick one to leave that single tool out, such as a server's tool that runs arbitrary code. Deleting or changing containers is never among them. **New endpoint** creates one and shows its token once, with a command to paste into an MCP client; **Create token** or **Replace token** makes a new one and ends the old. A container's own endpoint, made when it was created, has no token until you create one; the administrator's MCP token opens every endpoint. [The MCP guide](MCP.md#mcp-endpoints-for-one-agent) lists every scope's tools.

A template can declare its MCP servers, which **Install** names and its **Advanced editing** shows with its script, so every container from the image has them.

## Save a container as a snapshot

Use **Snapshot** after installing applications or changing settings in a container. Bilskirnir briefly pauses the container and uses Docker commit to save its filesystem changes as a new image layer.

For example, take a snapshot after installing an editor, then create a new container from it to try a plugin. The new container starts with the editor already installed. Changes made in the new container do not change the saved image or the original container.

The image tree follows Docker ancestry. A snapshot is based on the image that originally created its container. Taking two snapshots of that same container produces siblings. To extend one of those snapshots, create a new container from it and snapshot the new container.

| Saved in the snapshot | Kept separately or not saved |
| --- | --- |
| Installed packages and application files in the container | Files in mounted volumes |
| Desktop settings and home files, unless mounted | Temporary files under `/run` and `/tmp` |
| Changes in the container's writable layer | Running processes, RAM and GPU state |

An image that declares a Docker `VOLUME` needs an explicit named-volume mount for that path when you create a container. This makes it clear which files will be excluded from snapshots.

## See what takes disk space

**Images** shows each image's size as Docker reports it. Images share the layers they have in common, and Docker keeps a shared layer once, so a snapshot's size includes its whole base desktop while what it adds is often much smaller; for an image that shares layers, that part is shown under the size as **own**. Select a downloaded image's name to see what its space is made of: the steps of its history, newest first, grouped by the image they came from, each with the bytes it wrote and the command, copied files or setting behind it. A snapshot's own step holds the files that changed in its container, and a script build's largest step is usually its setup script; files a step deletes still take space in the earlier step that wrote them. Steps that only change settings write nothing and stay hidden until you show them. With the containerd image store, which Docker 29 and the WSL package use, Docker also keeps a compressed copy of each layer, and the size it reports includes those copies.

**Delete** on an image's row deletes it once you confirm. It cannot be undone; export an archive first if you may want the image again. A build, a snapshot or an imported archive goes from the list, and Docker deletes the image unless another image still holds it, which the notice says; images built or saved from it keep working, because Docker keeps the layers they share. An image registered from the Docker host was there before Bilskirnir, so only its entry in the list goes. On a downloaded catalog image, **Delete** removes the download: the image stays listed and downloads again the next time a container or a build uses it. An image a container was created from stays, since resetting that container needs it: the dialog names the container, so delete the container first.

## Ask for values when a container is created

A compose makes an image ask for a few values each time a container is created from it, such as the account to sign in with, and runs a short setup script with them before the container is yours. Under **Images**, select **Edit compose**, the compose button on the row of a snapshot, script build or imported image. Add a parameter for each value:

- **Name** is the environment variable the script reads, such as `UNITY_EMAIL`.
- **Label** is what **New container** asks for. A **Description** appears under the field, for help such as where to find the value.
- **Type** decides what the field takes: **Text**, a **Number** with an optional minimum, maximum and step, **On or off**, **One choice** from a list of options, **Several choices**, a **Volume** or a **Folder**. **Shown as** picks the control: a text area for long text, a slider for a number with a range, a checkbox instead of a switch, or radio buttons instead of a menu. **Allow other values** lets one choice also take text that is not an option.
- **Secret** makes a text field a password field. Its value is masked in job logs, never shown again and cannot have a default.
- **Pattern** makes a text value follow a format: a regular expression the whole value must match, such as `[A-Za-z0-9.-]+:[0-9]{1,5}` for a host and a port. **Expected format** describes it, such as `host:port, for example 192.168.1.20:10080`. **New container** shows it under the field when the value does not match, and creates nothing until it does; an empty value is still allowed unless the field is **Required**. The manager reads the pattern as Rust does, without lookaround or backreferences, and refuses one it cannot read when you save the compose. When the browser cannot read a pattern, the editor says so and only the manager checks it.
- **Required** values must be filled in; an **On or off** value that is required must be turned on, as for accepting a license. The **Default** fills in the field with the same control the form shows.
- **Volume** keeps files that every container of the image needs in one place, such as Unity installs, in a named volume instead of the image or the container. **Path in the desktop** is the folder the volume is mounted at, such as `/opt/unity`. As the **Default**, **No volume** leaves the folder in the container, a named volume shares it, and **A new volume for each container** gives every container one of its own, such as a workspace that should outlive a reset but not be shared.
- **Folder** names a folder of the desktop the script works in, such as the Unity project `/home/desktop/UnityProjects/MyGame`: an absolute path outside the system folders. The script creates what it needs there; the manager does not create the folder.

Whatever the type, the script receives text: a number as `42`, on or off as `true` or `false`, and several choices one per line, which `while read -r item; do ...; done <<< "$PLATFORMS"` reads one at a time. A volume parameter is the exception: the script reads its path, `/opt/unity`, and the name of the chosen volume from the same name with `_VOLUME` appended, such as `UNITY_HOME_VOLUME`, which is empty without a volume.

The upload dialog offers a container's volume and folder parameters by their labels under **From the image**, so files go straight into the folder the setup script prepared. A volume parameter's folder is offered with or without a volume.

In **New container**, a volume parameter lists the named volumes. **Create new volume…** creates one right there, named after the parameter unless you change it, and chooses it; it works when there are no volumes yet. **No volume** leaves the folder inside the container, where a reset discards it and snapshots keep it. Either way the folder exists and belongs to the desktop user before the script runs, so a script installing into `$UNITY_HOME` works without root. The container's details list the volume under **Volumes**; it stays mounted after a reset or a settings change, and the volume settings cannot mount it at a second path or mount another volume at its path.

When the compose gives each container a new volume, **New container** starts with **A new volume for this container** chosen. Nothing is created while you fill in the form: the volume appears with the container, named `bilskirnir-` followed by the container's ID and the parameter's name, such as `bilskirnir-0f1e…e1f0-workspace`, so a cancelled form leaves nothing behind. You can still pick an existing volume, or create one by name, instead. Unless the compose requires a volume, **No volume** is offered too, for a container whose folder may go with it: the folder stays inside the container, a reset discards it and deleting the container leaves no volume behind. A container created from a snapshot of that container gets a new volume too, not the one of the container the snapshot came from.

For example, an image with Unity Hub and an editor installed could ask for a Unity account and a serial number, and activate the license with them:

```bash
set -eu
editor=$(ls -d ~/Unity/Hub/Editor/*/Editor/Unity | head -n 1)
"$editor" -batchmode -quit -nographics -logFile - \
    -username "$UNITY_EMAIL" -password "$UNITY_PASSWORD" -serial "$UNITY_SERIAL"
```

with the parameters `UNITY_EMAIL` (required), `UNITY_PASSWORD` (required, secret) and `UNITY_SERIAL` (secret). The script runs as the desktop user unless **Run as root** is on.

When you choose such an image in **New container**, its fields appear below the image. The creation job waits until the desktop is ready, runs the script and finishes when the script does; its log shows the script's output. If the script fails, the container keeps running so you can look at it, and **Retry setup** in its details runs the script again. A reset runs it again too, since the reset container starts from the image's files.

The container keeps the compose it was created with, so editing the image's compose later affects only new containers. A snapshot of the container keeps that compose: a container created from the snapshot asks for the values again, so write the script so it can run where it already ran. Whatever the script writes, such as a license file, is part of the container's files and therefore of its snapshots.

**Build image** has a **Compose** tab that gives the new image a compose, and **Export configuration** and image archives carry it along.

## Move an image to another installation

Under **Images**, the archive button on an image's row (**Export archive**) downloads it as a `.tar.gz` file, with its name, description and desktop information inside. A script build's configuration travels along, so **Export configuration** still works after the move. Snapshots, script builds and imported images can all be exported. The archive holds every layer of the image, the base desktop included, so expect several gigabytes. Docker prepares the whole image before the download starts to grow; **Jobs** shows the export and any error.

On the other installation, select **Import image** and drop the file on the **Archive file** tab. The name and description from the archive fill in, and you can change them. The import is checked while it uploads: it must hold one Bilskirnir desktop image and stay within `IMAGE_ARCHIVE_MAX_MB` (64 GiB by default) once unpacked. Tags and image names inside the archive are ignored, so an archive cannot replace a catalog image or another image. The image appears with the source **Archive** once Docker has loaded it.

A `.tar` saved with `docker save` imports the same way, as long as it holds a desktop image. Volumes are never part of an archive; copy their files separately.

## Keep files in volumes

Create a named volume under **Volumes**, then mount it when creating a container. Use an absolute path such as `/workspace/project` or `/opt/game`. Protected system paths, read-only mounts over the desktop's home or configuration and the manager's own data volume, which holds its database, are rejected. **Volumes** labels that volume **Manager data** and offers no file browsing or export for it, and the volume choices of a mount or of a compose's volume parameter leave it out.

**Volumes** lists only the volumes Bilskirnir created, imported or copied, which carry its label; volumes that other software made on the same Docker Engine stay hidden and cannot be mounted or opened. A container that already mounts such a volume keeps it. The installation's `VOLUME_FILTER` setting chooses the label, or shows every volume with `none` ([installation settings](INSTALLATION.md#settings)).

The desktop user runs as UID/GID **1001:1001**. Existing volumes need permissions that allow this user to work with their files.

**Export archive** on a volume's row downloads its files as a `.tar.gz`, and **Import volume** unpacks such an archive, or a `.tar`, into a new volume of the name you choose, for a backup or a move to another installation. An import never replaces an existing volume, and its files belong to the desktop user. **Jobs** shows both transfers.

**Copy** on a volume's row copies all of its files into a new volume of the name you choose, for work that starts from a prepared project or cache, or a copy kept before a risky change. The copy keeps folders, symlinks and modes, and its files belong to the desktop user. It is a full copy, not a shared one: it takes as much disk space as the volume, and deleting a volume later does not shrink the WSL virtual disk by itself. **Jobs** shows the bytes copied.

**Browse files** on a volume's row, or its name, opens its files without starting a container. Open a folder to look inside, a text file of up to 512 KiB to read and change it, and use the row menu to download, rename or delete; a folder downloads as a `.tar.gz`. **New folder** and **Upload** add to the open folder; files dropped on the list upload too. Everything runs as the desktop user, so a file that user may not change is refused. A new, empty volume is given to the desktop user on its first change, so files can be added at its root.

Mounting the same writable volume in two containers gives both access to the same files. Taking a snapshot does not copy that volume. Deleting a container preserves named volumes and saved images; unsaved changes left only in the container are lost.

A volume that a compose created for one container says so under its name on **Volumes**: **Created for** links to that container, and **Owner deleted** marks one whose container was deleted while the volume stayed. When you delete a container that has such volumes, the delete dialog lists them under **Also delete the volumes created for this container**, which is off unless you turn it on. Turned on, it deletes those volumes and their files with the container, but never a volume you chose by name, and never one another container mounts.

**Delete** on a volume's row deletes that volume and every file in it once you confirm, such as one marked **Owner deleted** or any other volume you no longer need. It cannot be undone; export an archive first if you may want the files again. A volume that a container mounts, through its volumes or its compose, stays: the dialog names the container, so remove the volume from its settings or delete the container first. **Manager data** has no **Delete**, and a volume that a container outside Bilskirnir still uses is refused as well.

## Upload files from the browser

Drop files onto a running container card, the connected desktop or the upload dialog. You can also select **Upload files** to use the file picker.

The destination starts at the folder you uploaded into last in that container, or **Downloads**. Type another folder, or select **Browse…** to pick one: the list on the left holds the home folder with **Desktop**, **Documents** and **Downloads**, the folders the image's compose names, the writable volumes and the folders used last, and the address field opens any path you type. **New folder** creates a folder in the one that is open. Files can go anywhere in the home folder `/home/desktop`, into a writable volume or into a folder the compose names; folders under a read-only volume and system folders cannot be chosen. A folder that does not exist yet is created by the upload. Symlinks are listed but not followed. You can upload multiple files. Each file shows progress and a final status. Failed or cancelled files can be retried. Cancelling the queue keeps files that have already finished.

Existing files are preserved by default. Select **Replace existing files** when you intend to overwrite them. Uploads run as the desktop user, so a volume must be writable by UID/GID 1001:1001.

The home folder is part of the container filesystem unless a volume is mounted there. Use a named volume for files that should survive container deletion; the dialog says when the chosen folder is in a volume, which snapshots leave out. The default limit is 1 GiB per file, configurable with `UPLOAD_MAX_MB`.

## Download files to the browser

Select **Download files** in a running container's actions menu, its details or the connected desktop's toolbar. The dialog browses the desktop with the same places and address field as the upload folder picker. Select a file to download it as it is, up to the `UPLOAD_MAX_MB` limit. The button beside a folder, or **Download this folder** for the open one, downloads it as a `.tar.gz` archive named after the folder. The archive is made while it downloads, so the browser cannot show its size in advance; **Jobs** shows how much has been sent and any error. Files are read as the desktop user, and a file that user may not read fails the download rather than leaving a gap in the archive. Symlinks are kept as links, not followed.

You can also use Docker's file-copy commands from WSL. Replace `CONTAINER_ID` with your container's ID:

```bash
docker cp ./project bilskirnir-CONTAINER_ID:/workspace/project
docker cp bilskirnir-CONTAINER_ID:/home/desktop/Downloads/result ./result
```

## Browse and edit a container's files

**Files** in the sidebar browses a running container's files as the desktop user. Choose the container at the top; the address keeps the choice, so a link or a reload opens the same one. The page starts in the home folder and lists the same places as the upload folder picker beside the files, with an address field for any folder the desktop user can open. Open a folder to look inside, and a text file of up to 512 KiB to read and change it; a larger file downloads instead. Scripts (`.sh`, `.bashrc` or a shell `#!` line) and `.json` files open with their code colored. Saving replaces the file in one step and keeps its permissions, so a script stays executable. The row menu downloads, renames or deletes, and a folder downloads as a `.tar.gz`. **New folder** and **Upload** add to the open folder, and files dropped on the list upload too, wherever the upload rules allow; elsewhere the page says why they are off. The manager refuses anything the desktop user may not do, such as changing a system file.

## Change a container's settings

Open a container's actions menu and select **Settings** to change its name, resolution, limits, GPU, internet, sudo, sound, microphone, gamepad, volumes or machine identity. The image cannot change; take a snapshot or reset the container instead.

**Machine identity** shows the hostname, MAC address and machine ID that software inside the desktop sees, such as a license bound to one machine. A container keeps them through resets, retries and settings changes, while a new container, also one from a snapshot, gets its own. Change them only to let this container take over the identity of one it replaces; they can be changed but not cleared. A container created before Bilskirnir kept identities shows them empty until it is next recreated, and keeps the machine ID of its image until one is set here. The details, including which Docker releases keep the internet interface's MAC address, are in [the API guide](API.md#machine-identity).

The name, internet access and a new CPU or memory limit apply while the container keeps running. Docker fixes the other settings, and removing a limit, when it creates a container, so changing them creates a new one: Bilskirnir stops the container, saves its files to an image, starts a new container from it with the new settings and removes the old one. The dialog says so before applying. Open applications close, but installed software and files in the container are kept, as are named volumes. If the new container cannot start, the previous one is restored.

## Internet, sound and microphone

The internet switch attaches or detaches the container's Docker egress network. Its private connection to the manager stays in place, so you can keep using the desktop while internet access is off. This is a Docker network setting, not a general firewall policy for the host or LAN.

Sound and microphone must be enabled both for the installation and for the container. Use the browser toolbar controls to start each channel. Microphone input also needs browser permission. Localhost supports this over HTTP; access from another device requires HTTPS. Leaving the desktop closes the media connections and microphone tracks.

## Read job results

Longer operations appear under **Jobs**. Open a job to see its status and output; the panel updates while work continues. Image builds show script output and list their stages: preparing the base image, running the script, saving the image layer, writing and tagging the image and cleaning up. The current stage is highlighted with its running time, each finished one shows how long it took, and the job list names the current stage. Only a base image download has a progress bar; Docker reports no progress while the script runs or the layer is saved, so a large change can sit in **Saving the image layer** for minutes. Snapshots and settings changes that recreate a container list their stages too, and a large container layer can sit in **Committing the container layer** for minutes for the same reason. Image downloads show a progress bar with the downloaded size, and the job list shows their percentage. A container or an image that is still being made already appears under **Containers**, on the dashboard's table and under **Images**, with its name, how far its job is and **View log** for the job panel, but no controls until it is ready: a container until its desktop runs, a saved image until its record is written. A catalog image that is downloading or building shows its job the same way instead of its download and build buttons. Deletion jobs explain which container and networks were removed and confirm that named volumes and saved images were preserved, or name the volumes created for the container that were deleted with it.

If container creation failed partway through, **Retry setup** can complete missing setup without replacing an existing container. Agents can also reset a container through the API, which recreates the container from its saved image while keeping named volumes. See [recovery](INSTALLATION.md#when-setup-is-interrupted) for what can and cannot be restored.
