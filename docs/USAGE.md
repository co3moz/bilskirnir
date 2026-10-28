# Using Thoric

[Back to the README](../README.md)

A container is an Ubuntu desktop that Thoric runs in Docker. An image is the starting point for a container. A compose belongs to an image and predefines what a new container from it asks for and sets up. A volume holds files separately from the container, so you can keep using them after the container is gone.

## Find your way around

The sidebar leads to **Dashboard**, **Containers**, **Images**, **Templates**, **Volumes**, **MCP**, **Files** and **Management**, lists the running containers for a quick connect and holds **New container** with a hammer button for **Build image**. The release this installation runs stands beside the name at its top. On a phone it opens from the button at the top left. The menu at the bottom of the sidebar switches between the dark, light and system theme, chooses the language and signs out. When a newer release of Thoric is out, a card above that menu names it and links to its notes; in the WSL package its **Update** button installs it, see [Update from the dashboard](WSL_PACKAGE.md#update-from-the-dashboard). **Jobs** sits at the top right of every page, the desktop view included, with the number of jobs in progress on it; it opens the latest jobs, and **All jobs** leads to the **Jobs** page. A job opens with its log, and **Copy log** copies it as far as it has been written, also while the job runs. While **Management** or one of its pages is open, **Jobs**, with the same number, shows below it in the sidebar, and while **MCP** or **Activity** is open, **Activity** shows below **MCP**.

The dashboard speaks English, German, French, Spanish and Turkish. It follows the browser's language, or English when the browser prefers another one; a language picked in that menu, or on the sign-in page, is kept in this browser. Messages from the manager itself, such as job logs and errors, stay in English.

The **Dashboard** sums up the host: how many containers exist and run, jobs in progress and today's failures, and the processors and memory Docker reports with the limits running containers use. **Resource usage** follows each running container once a second: CPU in cores, memory, disk reads and writes and network traffic, one color per container and stacked, so the top edge is what they use together, for the last 5, 15 or 30 minutes. A table under the charts gives each container's current figures. A deleted container stays in the charts, marked as deleted and in its color, until its samples are 30 minutes old. While a GPU container runs, the host's NVIDIA GPU is charted beside them; that is the whole GPU, the host's other programs included, in WSL Windows applications too, since neither WSL nor the driver reports which container uses it. AMD and Intel GPUs have no such chart. The history is kept in memory only and starts over when Thoric restarts. A container's details panel shows its own charts against its CPU and memory limits. Below that, a chart shows finished and failed jobs per day, and a table lists every container with tabs for running, stopped and those that need attention. Select several rows to start or stop them together. Select a name to open its details panel. **Containers** shows the same containers as cards with a live preview.

## Create and connect

Select **New container**, give it a name and choose an image: XFCE or GNOME from the catalog, a script build, a snapshot or an imported image. Under **Features**, buttons turn GPU access, internet access, sudo, sound, microphone and a virtual gamepad device for agents on or off; hover or focus one to see what it does. They start as the installation sets them (`GPU_DEFAULT` and the others in [the installation guide](INSTALLATION.md#settings)), and a feature the installation switched off is shown off with the reason below the buttons. **Advanced settings** holds the resolution, shared memory, CPU and memory limits, proxied ports and volume mounts; it starts closed with its current values in one line, and opens by itself when one of its fields needs a correction. **Proxied ports** lists ports of programs inside the desktop, separated by commas, that an agent holding the MCP token reaches through the manager, as [desktop ports](API.md#desktop-ports) describes; a change applies without a restart.

**Create an MCP endpoint**, on by default, gives the new container an endpoint of its own at `/mcp/<name>`, where an agent reaches only this container and the MCP servers running in it; see [MCP servers and endpoints](#mcp-servers-and-endpoints).

After creation finishes, select **Connect**. The card preview is a screenshot; the connected desktop accepts keyboard and mouse input. Use the container's actions menu or its details panel to stop, restart, change settings, inspect or delete it. A desktop's address, such as `#/desktop/<id>`, can be bookmarked.

The GPU option is available only when the installation enables `WSL_GPU` or `NVIDIA_COMPUTE`. There is no fixed container quota. Leaving CPU and memory limits empty uses Docker's unlimited defaults, so choose limits according to the machine and applications you are running. In the WSL package, all containers share the memory WSL gives its virtual machine; [Memory, processors and disk](WSL_PACKAGE.md#memory-processors-and-disk) explains how to raise it. Enabling GPU access does not reserve GPU memory or set a VRAM limit.

## Choose XFCE or GNOME

Both catalog images are Ubuntu 24.04. XFCE uses fewer resources and is a good starting point for application work. GNOME provides the Ubuntu shell and Yaru theme when you prefer that desktop.

The GNOME image runs an X11 session in the container. It does not include GDM, a Wayland login session or host power and USB management. Screen locking is disabled; access goes through the manager's login.

**Images** lists them with the source **Catalog**, above the images you save. A catalog image is downloaded the first time you create a container or build an image from it. The job panel shows the download's size and progress, and later containers from it start without it. On its row, the download button fetches it ahead of time and, once it is here, the update button fetches its latest published release. `DESKTOP_IMAGE` sets the default selection. Script builds and snapshots made from a catalog image appear under it and keep its desktop information.

## Running graphics applications

The desktop uses TigerVNC/Xvnc for its display. GPU applications use VirtualGL's EGL back end: in WSL Mesa's D3D12 driver renders on the WSL GPU, on a Linux host NVIDIA's own driver through the NVIDIA Container Toolkit, or Mesa's own drivers on the `/dev/dri` render devices of AMD and Intel GPUs (experimental); see [GPU on a Linux host](INSTALLATION.md#gpu-on-a-linux-host). In a GPU-enabled container, use the supplied `gpu-run` wrapper:

```bash
gpu-run glxinfo -B
gpu-run /path/to/Unity -force-glcore
gpu-run /path/to/MyLinuxGame -force-glcore
```

The Unity and game commands are launch examples, not confirmation that a particular application version works. Install your Linux application first and test its graphics requirements. Windows executables and Wine are not included.

Chrome, Edge and other Chromium-based browsers, and Electron applications, keep their own sandbox. Containers can create the user namespaces those sandboxes are built on, so these applications start from the menu, a terminal or a link another application opens in the default web browser, and `about:sandbox` reports the namespace sandbox. An installation with `DESKTOP_USER_NAMESPACES=false` keeps Docker's default seccomp profile instead: these applications then exit at startup unless they get `--no-sandbox`, and a link opened in such a default browser fails with "Failed to execute default Web Browser". A container created before an upgrade keeps the seccomp profile it was created with until a reset or a settings change recreates it.

In a GPU-enabled container these browsers and applications use the GPU however they are started: the web browser button in the panel, the applications menu, a link another application opens or a terminal, in XFCE and GNOME, with no flags and no `gpu-run`. In WSL `about:gpu` then reports hardware acceleration for WebGL, WebGPU, canvas, compositing and video decode through `ANGLE (D3D12 (NVIDIA ...))`. Video encode stays in software, and Vulkan stays disabled because the image has no Vulkan driver for the WSL GPU. Chrome runs its GPU process outside the sandbox by itself, so the WSL GPU device stays reachable while pages stay sandboxed. Without GPU access the same browsers start normally and render in software.

This needs `VGL_CHROMEHACK=1`, VirtualGL's documented workaround for the way Chromium's GPU process picks its X visual. The desktop session runs through `gpu-run`, which turns it on for each program whose executable sits beside Chromium's `icudtl.dat` and `resources.pak`, whatever the program is called and however it was installed. Other programs, including those a browser starts, keep VirtualGL's default: the workaround attaches the browser's 32-bit visual to every OpenGL configuration with an alpha channel, which would change the window visual of a game or Unity player that requests one. An explicit `VGL_CHROMEHACK` always wins, so `VGL_CHROMEHACK=1 gpu-run <command>` turns it on for any program and `VGL_CHROMEHACK=0` turns it off for a browser. A browser started outside the desktop session, such as from a script job or a terminal session of the API, needs `gpu-run` like any other GPU application.

GPU-enabled startup checks the renderer and rejects software rendering. If a GPU container fails to start, open **Logs**: the renderer check is written to the container log before the desktop exits, and **Diagnostics** needs a running desktop. The check names the setting to look at by the devices the container got. Use **Diagnostics** when the desktop runs but rendering does not work; it also lists the GPU devices and the EGL devices VirtualGL can choose. Hardware OpenGL has been verified on an NVIDIA RTX 4080 in WSL2. The NVIDIA and AMD/Intel routes on a Linux host, Unity itself and Vulkan have not been verified.

`WSL_GPU`, `NVIDIA_COMPUTE` and `DRI_GPU` allow the installation's routes to the GPU, and the manager uses those the host has. In WSL, turning `NVIDIA_COMPUTE` off leaves WSL graphics without CUDA. `GPU_ADAPTER` picks the GPU: in WSL a Mesa D3D12 adapter by vendor or part of its model name, such as `NVIDIA`, `AMD` or `Intel`, and with `/dev/dri` the `AMD` or `Intel` card. Each container still has its own GPU setting.

## Make an image with a script

Under **Images**, select **Build image**, choose a desktop base and enter a shell script. The script runs as root during the build, which lets you install packages and applications. Enable build internet access if your script needs to download them. The job log shows the script's output and the resulting image.

For example, this installs a couple of tools on either catalog image:

```bash
set -eu
apt-get update
apt-get install -y --no-install-recommends git curl
rm -rf /var/lib/apt/lists/*
```

Keep the desktop startup, VNC and GPU components in place when adding software. The result must still start as a Thoric desktop.

**Build image** also takes an optional description and an icon. Choose any picture, PNG, JPEG, WebP, GIF or SVG; it is scaled down to fit 256 by 256 pixels and saved as a PNG. **Images** shows the icon beside the name, and selecting it, or the plain image symbol of a saved image without one, changes or removes it. Snapshots keep the icon of the image their container came from.

To reuse a build, select **Export configuration**, the copy button on its row under **Images**. It copies the image's name, description, base image, internet setting, script and icon to the clipboard as JSON. In **Build image**, **Import configuration** takes that JSON and fills in the form, so you can change the name or base image before building, here or on another installation. A base image that is not available on this host is pointed out instead of selected. Snapshots and imported images have no script, so they have no configuration to export, and builds from an earlier release did not record theirs. [An archive](#move-an-image-to-another-installation) moves any of them.

## Install a template

**Templates** lists ready-made images from the [Thoric wiki](https://github.com/co3moz/thoric/wiki), each with its icon, a description and tags; search, or pick a tag in the menu beside the search or on a card, to narrow the list. The menu offers the tags several templates share, each with its count; a tag only one template carries is found by the search. **Applications** come first: desktop programs, most with an MCP server an agent drives them through. **Environments** follow below them: language toolchains such as Node.js, Python or Rust, which an agent builds and tests code with through Thoric's terminal, command and file tools. An environment is meant to be merged with an application, such as Node.js with VS Code, with **Merge and install** below; it also installs on its own. Search, tags, **Merge and install** and **Installed** work across both sections. **Install** opens **Build image** with the template filled in, asking only for a name and a base image: the template's description, icon and MCP servers are shown as they are, and selecting **Build image** builds it with its own script and compose. A template with options lists them under its description, each checked or not as the template suggests, such as optional tools: clear one to leave it out of the image, or check one to add it, and an option's MCP servers come and go with it. **Advanced editing** opens the whole form, where you can read the script before building, since it runs as root, and change it, the description, the icon or anything else. The options show there above the script, which gets their values as lines such as `WINE=false` at its top when the image builds. The build then runs like any other, and the image appears under **Images**. A container created from it asks for the compose's values, such as an account, as usual.

To have several templates in one image, such as VS Code with the Node.js environment, select **Merge and install**. The cards turn into choices: select each template you want, on its card or its checkbox, in the order its script should run; each chosen card shows its step number. **Merge** becomes available once two are chosen and opens **Build image** with all of them, asking again only for a name and a base image. Its **Advanced editing** shows every template's script in a section of its own, which you can read, change, run earlier or later, or leave out, and a compose with a section for each template that asks for values. Templates with options list them under their names, both there and in the short form. Two templates may ask for the same value, such as a version each; a container created from the image asks for each template's values in a section of its own, and each script receives only its own. The image is built on the first template's base image, and the form warns when another template was made for a different one. **Cancel** leaves the choices.

A template you have built shows **Installed**, also when it was merged with others, and **Install again** builds another image from its current version. A template that needs a newer Thoric says which release it needs instead of offering **Install**. Your browser reads the list directly from GitHub, so it needs internet access, and the page sends no request until you open it. An installation can point the page at another list or turn it off with `TEMPLATES_URL`; see [the installation guide](INSTALLATION.md#settings).

**Import image** with its **Docker image** tab registers a compatible image that is already present in the host Docker Engine. It does not turn an arbitrary server image into a desktop. An image removed from Docker cannot be used to create another container until it is available again.

## MCP servers and endpoints

A desktop can run MCP servers of its own, such as a browser automation server a template brings, or one you write while developing a Chrome extension. The **MCP** page shows them under **Desktop servers**, grouped by container: where each came from (**Image** for a template's, **Added** for one added later), its state, its tools and the end of its output. **Add server** registers one in a running container: a command that speaks MCP on stdio, or the address of an HTTP server listening inside the desktop. **Test** starts it once in that container without saving it and shows its name and tools, or why it did not answer with the end of what it wrote, so a wrong command or a missing value shows before the server is added. Inside the desktop, `thoric-mcp add NAME -- COMMAND ARGS` does the same from a terminal, and the Thoric icon in the panel's tray lists the desktop's servers with their states: **Manage MCP servers** opens a window that shows each server's tools, errors and output and the endpoints that reach the desktop, and adds, edits and removes servers by running `thoric-mcp`, whose command line it shows so you can repeat it in a script. Thoric starts a server as the desktop user only when its tools are needed and never opens a port for it. A value such as `${API_TOKEN}` in its arguments or headers is filled in from the container's compose values.

**Endpoints** lists the addresses agents connect to instead of the administrator's `/mcp`: each covers one or several containers, and for each only the groups of tools you tick, such as screenshots, mouse and keyboard or files, and its servers, all of them or the ones you pick. Each ticked group and each included server shows its tools behind their count; untick one to leave that single tool out, such as a server's tool that runs arbitrary code. Deleting or changing containers is never among them. **New endpoint** creates one and shows its token once, with the settings of each MCP client to copy; **Create token** or **Replace token** makes a new one and ends the old. A container's own endpoint, made when it was created, has no token until you create one; the administrator's MCP token opens every endpoint. [The MCP guide](MCP.md#mcp-endpoints-for-one-agent) lists every scope's tools.

The share button beside an endpoint's URL, or **Add to a client** in its menu, and **Add /mcp to a client** for Thoric MCP itself, show what each client needs to reach it: Claude Code, Codex, VS Code with GitHub Copilot, the Copilot CLI, Cursor, OpenCode, Gemini CLI, Devin Desktop (formerly Windsurf), Zed, Cline, Kiro, Amp and Claude Desktop, or any other client. Pick one to get its command or the entry for its settings file, with the files it goes into; VS Code and Cursor also get a button that opens the client to add the server. The dialog remembers the client you picked last. Paste the token into the dialog to fill it in, since the manager keeps only its hash, or turn the Authorization header off for an installation with `MCP_AUTH_MODE=none`. The token stays in the page and is not sent anywhere. Claude Desktop starts only local programs, so it gets the stdio bridge through WSL or `mcp-remote` instead of the URL.

A template can declare its MCP servers, which **Install** names and its **Advanced editing** shows with its script, so every container from the image has them.

### Test a server in the inspector

**Inspector** opens one server and shows what it offers: a desktop server of a running container, an endpoint as its agent sees it, or Thoric MCP itself. Choose it and press **Connect**, or use the play button of a server under **Desktop servers** or **Test in the inspector** in an endpoint's menu. The server's name, protocol revision, capabilities and instructions show above its tools, prompts, resources and resource templates, which you can search by name, title or URI and sort.

Choosing one shows its description, its hints such as read-only or destructive, and a form made from its input schema: nested objects, lists you can add to, reorder and remove from, and variants picked by the property that tells them apart, such as `type`. **JSON** edits the same arguments as text, in the editor of VS Code: it completes property names and values from the schema (Ctrl+Space), explains a property when you point at it and underlines what the schema refuses, as the JSON fields of the form do. On a touch screen, where that editor does not work, they are plain text fields. **Run tool**, **Get prompt** or **Read resource** sends them (Ctrl+Enter), shows the server's progress, and **Cancel** (Esc) asks the server to stop. The result shows each part as it is meant to be read: text, JSON, images, embedded resources and links, `structuredContent` and the raw result, which you can copy or save; audio is offered to save, since the page plays nothing from a server. The play button beside an item runs it again with its last arguments. **Traffic and logs** lists every message to and from the server, what it wrote on its standard error and its log messages, newest first.

A desktop server runs in a process of its own for the inspector, as the desktop user, and stops when you disconnect or leave the page; agents keep their own connections to it. A server that locks something of its own, such as a browser profile, may not start a second time while an agent uses it.

### When an agent asks for you

Some steps are for a person only, such as a CAPTCHA, a second sign-in factor, a license window or a sign-in screen. An agent that reaches one asks with [`request_human`](MCP.md#ask-a-person-for-help) and waits. The request appears at the top of every dashboard page, with the container's name, what the agent needs, how long ago it asked and, when it came through an endpoint, that endpoint's name, and a notice in the corner says so on whichever page is open. **Open desktop** takes you to that desktop, where the same request sits in a strip under the toolbar. Do what is asked there, then press **Done**, or **Decline** when you will not or cannot, with a short reply for the agent if it helps, such as which account you signed in with. The agent goes on as soon as you answer. The first answer counts, from whichever page or tab gives it, and the request leaves every page. Requests are kept in the manager's memory only: restarting the manager ends them, and an agent that still needs you asks again.

## See what agents did

**Activity** lists every tool call agents made over MCP, and every transfer link they used, newest first and grouped by day: when it happened (point at the time for the exact moment), who called, the tool, the container, the arguments that say most about it, such as a script's first line or a path, how it ended and how long it took. The caller is **Thoric MCP** for `/mcp` or the endpoint's name, with a mark when the call came from the inspector above, which is you rather than an agent, or through a transfer link. A destructive call has a red mark, a change a pink one and a read a grey one, each with its word, and a failed or refused call a red badge. Filter by container, by caller or by part of a tool's name, or show only errors or only destructive calls; the list follows new calls as they come. Select a call to see all its arguments, its error and the job it started.

What an agent typed, wrote into a file or passed as a secret or a token shows only as its size, as [the MCP guide](MCP.md#agent-activity) describes; `exec_container`'s script is kept. A container's details panel shows its latest calls, and **Agent activity** in an endpoint's menu on the **MCP** page opens the list for that endpoint. The newest 10,000 calls are kept.

## Save a container as a snapshot

Use **Snapshot** after installing applications or changing settings in a container. Thoric briefly pauses the container and uses Docker commit to save its filesystem changes as a new image layer.

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

**Delete** on an image's row deletes it once you confirm. It cannot be undone; export an archive first if you may want the image again. A build, a snapshot or an imported archive goes from the list, and Docker deletes the image unless another image still holds it, which the notice says; images built or saved from it keep working, because Docker keeps the layers they share. An image registered from the Docker host was there before Thoric, so only its entry in the list goes. On a downloaded catalog image, **Delete** removes the download: the image stays listed and downloads again the next time a container or a build uses it. An image a container was created from stays, since resetting that container needs it: the dialog names the container, so delete the container first.

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

The upload dialog offers a container's volume and folder parameters by their labels under **From the image**, so files go straight into the folder the setup script prepared; an image of several merged templates lists them under each template's name instead. A volume parameter's folder is offered with or without a volume.

In **New container**, a volume parameter lists the named volumes. **Create new volume…** creates one right there, named after the parameter unless you change it, and chooses it; it works when there are no volumes yet. **No volume** leaves the folder inside the container, where a reset discards it and snapshots keep it. Either way the folder exists and belongs to the desktop user before the script runs, so a script installing into `$UNITY_HOME` works without root. The container's details list the volume under **Volumes**; it stays mounted after a reset or a settings change, and the volume settings cannot mount it at a second path or mount another volume at its path.

When the compose gives each container a new volume, **New container** starts with **A new volume for this container** chosen. Nothing is created while you fill in the form: the volume appears with the container, named `thoric-` followed by the container's ID and the parameter's name, such as `thoric-0f1e…e1f0-workspace`, so a cancelled form leaves nothing behind. You can still pick an existing volume, or create one by name, instead. Unless the compose requires a volume, **No volume** is offered too, for a container whose folder may go with it: the folder stays inside the container, a reset discards it and deleting the container leaves no volume behind. A container created from a snapshot of that container gets a new volume too, not the one of the container the snapshot came from.

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

On the other installation, select **Import image** and drop the file on the **Archive file** tab. The name and description from the archive fill in, and you can change them. The import is checked while it uploads: it must hold one Thoric desktop image and stay within `IMAGE_ARCHIVE_MAX_MB` (64 GiB by default) once unpacked. Tags and image names inside the archive are ignored, so an archive cannot replace a catalog image or another image. The image appears with the source **Archive** once Docker has loaded it.

A `.tar` saved with `docker save` imports the same way, as long as it holds a desktop image. Volumes are never part of an archive; copy their files separately.

## Keep files in volumes

Create a named volume under **Volumes**, then mount it when creating a container. Use an absolute path such as `/workspace/project` or `/opt/game`. Protected system paths, read-only mounts over the desktop's home or configuration and the manager's own data volume, which holds its database, are rejected. **Volumes** labels that volume **Manager data** and offers no file browsing or export for it, and the volume choices of a mount or of a compose's volume parameter leave it out.

**Volumes** lists only the volumes Thoric created, imported or copied, which carry its label; volumes that other software made on the same Docker Engine stay hidden and cannot be mounted or opened. A container that already mounts such a volume keeps it. The installation's `VOLUME_FILTER` setting chooses the label, or shows every volume with `none` ([installation settings](INSTALLATION.md#settings)).

The desktop user runs as UID/GID **1001:1001**. Existing volumes need permissions that allow this user to work with their files.

**Export archive** on a volume's row downloads its files as a `.tar.gz`, and **Import volume** unpacks such an archive, or a `.tar`, into a new volume of the name you choose, for a backup or a move to another installation. An import never replaces an existing volume, and its files belong to the desktop user. **Jobs** shows both transfers.

**Copy** on a volume's row copies all of its files into a new volume of the name you choose, for work that starts from a prepared project or cache, or a copy kept before a risky change. The copy keeps folders, symlinks and modes, and its files belong to the desktop user. It is a full copy, not a shared one: it takes as much disk space as the volume, and deleting a volume later does not shrink the WSL virtual disk by itself. **Jobs** shows the bytes copied.

**Browse files** on a volume's row, or its name, opens its files without starting a container. Open a folder to look inside, a text file of up to 512 KiB to read and change it, and use the row menu to download, rename or delete; a folder downloads as a `.tar.gz`. **New folder** and **Upload** add to the open folder; files dropped on the list upload too. Everything runs as the desktop user, so a file that user may not change is refused. A new, empty volume is given to the desktop user on its first change, so files can be added at its root.

Mounting the same writable volume in two containers gives both access to the same files. Taking a snapshot does not copy that volume. Deleting a container preserves named volumes and saved images; unsaved changes left only in the container are lost.

A volume that a compose created for one container says so under its name on **Volumes**: **Created for** links to that container, and **Owner deleted** marks one whose container was deleted while the volume stayed. When you delete a container that has such volumes, the delete dialog lists them under **Also delete the volumes created for this container**, which is off unless you turn it on. Turned on, it deletes those volumes and their files with the container, but never a volume you chose by name, and never one another container mounts.

**Delete** on a volume's row deletes that volume and every file in it once you confirm, such as one marked **Owner deleted** or any other volume you no longer need. It cannot be undone; export an archive first if you may want the files again. A volume that a container mounts, through its volumes or its compose, stays: the dialog names the container, so remove the volume from its settings or delete the container first. **Manager data** has no **Delete**, and a volume that a container outside Thoric still uses is refused as well.

## Upload files from the browser

Drop files onto a running container card, the connected desktop or the upload dialog. You can also select **Upload files** to use the file picker.

The destination starts at the folder you uploaded into last in that container, or **Downloads**. Type another folder, or select **Browse…** to pick one: the list on the left holds the home folder with **Desktop**, **Documents** and **Downloads**, the folders the image's compose names, the writable volumes and the folders used last, and the address field opens any path you type. **New folder** creates a folder in the one that is open. Files can go anywhere in the home folder `/home/desktop`, into a writable volume or into a folder the compose names; folders under a read-only volume and system folders cannot be chosen. A folder that does not exist yet is created by the upload. Symlinks are listed but not followed. You can upload multiple files. Each file shows progress and a final status. Failed or cancelled files can be retried. Cancelling the queue keeps files that have already finished.

Existing files are preserved by default. Select **Replace existing files** when you intend to overwrite them. Uploads run as the desktop user, so a volume must be writable by UID/GID 1001:1001.

The home folder is part of the container filesystem unless a volume is mounted there. Use a named volume for files that should survive container deletion; the dialog says when the chosen folder is in a volume, which snapshots leave out. The default limit is 1 GiB per file, configurable with `UPLOAD_MAX_MB`.

## Download files to the browser

Select **Download files** in a running container's actions menu, its details or the connected desktop's toolbar. The dialog browses the desktop with the same places and address field as the upload folder picker. Select a file to download it as it is, up to the `UPLOAD_MAX_MB` limit. The button beside a folder, or **Download this folder** for the open one, downloads it as a `.tar.gz` archive named after the folder. The archive is made while it downloads, so the browser cannot show its size in advance; **Jobs** shows how much has been sent and any error. Files are read as the desktop user, and a file that user may not read fails the download rather than leaving a gap in the archive. Symlinks are kept as links, not followed.

You can also use Docker's file-copy commands from WSL. Replace `CONTAINER_ID` with your container's ID:

```bash
docker cp ./project thoric-CONTAINER_ID:/workspace/project
docker cp thoric-CONTAINER_ID:/home/desktop/Downloads/result ./result
```

## Browse and edit a container's files

**Files** in the sidebar browses a running container's files as the desktop user. Choose the container at the top; the address keeps the choice, so a link or a reload opens the same one. The page starts in the home folder and lists the same places as the upload folder picker beside the files, with an address field for any folder the desktop user can open. Open a folder to look inside, and a text file of up to 512 KiB to read and change it; a larger file downloads instead. A file opens in the code editor build scripts use, colored by its kind: scripts (`.sh`, `.bashrc` or a shell `#!` line), `.json` and the other languages the editor knows by a file's name, such as Python, YAML or a `Dockerfile`. Configuration files such as `.conf`, `.env`, `.toml`, `.desktop` or a systemd unit color as INI, and shaders (`.glsl`, `.hlsl`, Unity's `.shader` and `.cginc`, Godot's `.gdshader`) as C++. **Ctrl+S** saves. Saving replaces the file in one step and keeps its permissions, so a script stays executable. The row menu downloads, renames or deletes, and a folder downloads as a `.tar.gz`. **New folder** and **Upload** add to the open folder, and files dropped on the list upload too, wherever the upload rules allow; elsewhere the page says why they are off. The manager refuses anything the desktop user may not do, such as changing a system file.

## Change a container's settings

Open a container's actions menu and select **Settings** to change its name, resolution, limits, GPU, internet, sudo, sound, microphone, gamepad, volumes or machine identity. The image cannot change; take a snapshot or reset the container instead.

**Machine identity** shows the hostname, MAC address and machine ID that software inside the desktop sees, such as a license bound to one machine. A container keeps them through resets, retries and settings changes, while a new container, also one from a snapshot, gets its own. Change them only to let this container take over the identity of one it replaces; they can be changed but not cleared. A container created before Thoric kept identities shows them empty until it is next recreated, and keeps the machine ID of its image until one is set here. The details, including which Docker releases keep the internet interface's MAC address, are in [the API guide](API.md#machine-identity).

The name, internet access and a new CPU or memory limit apply while the container keeps running. Docker fixes the other settings, and removing a limit, when it creates a container, so changing them creates a new one: Thoric stops the container, saves its files to an image, starts a new container from it with the new settings and removes the old one. The dialog says so before applying. Open applications close, but installed software and files in the container are kept, as are named volumes. If the new container cannot start, the previous one is restored.

## Internet, sound and microphone

The internet switch attaches or detaches the container's Docker egress network. Its private connection to the manager stays in place, so you can keep using the desktop while internet access is off. This is a Docker network setting, not a general firewall policy for the host or LAN.

Sound and microphone must be enabled both for the installation and for the container. Use the browser toolbar controls to start each channel. Microphone input also needs browser permission. Localhost supports this over HTTP; access from another device requires HTTPS. Leaving the desktop closes the media connections and microphone tracks.

## Management and updates

**Management** shows the release this installation runs and the newest one GitHub names, with when the manager last asked. The manager asks when it starts and every six hours; **Check now** asks at once, so a release published a minute ago shows without waiting. When a newer release is out, **Update to** installs it in the WSL package, as the sidebar card does, and **What's new** opens its notes. On another installation the page names the release, and you update it the way you installed it. `UPDATE_CHECK=false` turns the lookup off, and with it the button. A notice under the release says that Thoric is built from open-source packages and links to **Dependencies and licenses**.

Below that, **Configuration** lists the [installation settings](INSTALLATION.md#settings) the manager runs with, by section, each with what it does. The values are the ones the manager made of its environment when it started: an empty `TEMPLATES_URL`, for example, shows the wiki's list it reads instead. `ADMIN_PASSWORD` and `MCP_TOKEN` only say whether they are set; their values never leave the manager. Under **Container features** each feature's switch, such as `AUDIO_ENABLED`, stands beside its default for new containers, `AUDIO_DEFAULT`. While the switch is off its default gives nothing, and the default says so; where it has a control, that shows off and waits for the switch, keeping the value. The microphone's two settings wait for `AUDIO_ENABLED` too.

In the WSL package, most settings get a control: a switch, a number, a choice or a text field. A changed one is marked **Changed** with a button that undoes it, and a value the manager would refuse is marked with the reason. **Apply and restart** checks the changes with the manager, lists them with their old and new values and warns about running jobs, which end with the manager; then the package writes them to `.env`, keeping the previous file, and recreates the manager. The page shows the change running, reloads once the manager answers and asks you to sign in again; if the new manager does not answer, the package puts the previous settings back and the job says why. Sign-in, the root MCP and its token, the container name and the host name of the HTTPS service are marked **CLI only**: change them with `thoric settings` in a Thoric terminal, then `thoric restart`. See [Settings from the dashboard](WSL_PACKAGE.md#settings-from-the-dashboard). On another installation the page only shows the settings: change one in `.env`, then recreate the manager with `docker compose up -d`.

## See what Thoric is built from

**Dependencies** lists everything this release is built from, in three tabs. **Interface** has every package of the dashboard, **Backend** every crate of the manager, each with its version and license and whether it ships (**Runtime**), only builds (**Build**) or only serves the tools and tests (**Development**); **Direct** marks the ones Thoric names itself rather than through another package. **Infrastructure** has the images, system packages, tools, downloads and workflow actions of the manager image, the desktop images, the WSL package, Compose and CI, with where each is used. A system package without a version takes whatever its distribution has when the image builds. The filter keeps one kind, scope or license, the search looks through names, versions, licenses and descriptions, and a name opens its page on npm, crates.io, Docker Hub or GitHub. **Download JSON** saves the whole list.

The build of the manager image writes the list, so it is the list of the release you run and changes only with an update. A manager built another way, such as with `cargo run`, has none and says so.

## Back up or move the whole installation

**Management** has **Backup and restore**, which keeps the whole installation in one `.tar.gz` file or brings it into another installation. A backup always carries the containers with their settings and machine identity, their composes and values, and the MCP endpoints with their tokens. Tick what else it carries:

- **Saved images**: snapshots, script builds and imports with their icons and composes. Catalog images never travel; the other installation downloads them when it needs them.
- **Containers' files**: what is installed and saved in each container outside its volumes, so a container comes back as it was instead of as after a reset. Docker pauses a running desktop briefly while it saves its files.
- **Volumes**: the named volumes the containers mount and those you created here, never the manager's own.
- **Secret values**: the values of the composes' secret parameters, such as a license password. Leave it unticked unless the other installation needs them: the file then holds them in plain text. Without them, a reset runs the setup without those values.

**Download backup** starts the download at once; the backup is made while it downloads, nothing is stored on the manager first, and **Jobs** shows its progress. With images, files and volumes it can take tens of gigabytes. The file is not encrypted: keep it where only you can read it, and see [moving to another computer](INSTALLATION.md#move-to-another-computer) for the steps and for how the settings in `.env` move.

To restore, choose a backup file under **Restore a backup**. The dashboard reads what the file holds and lists the containers, images, volumes and endpoints it creates, what it does not carry, and anything this installation already has under the same name. A restore only adds: a name already in use stops it before anything is created, so delete or rename that item, or restore into a new installation. **Restore** uploads the file; keep Thoric open in this tab until the upload ends, then the job saves the records and creates the containers. They come back stopped, with their hostname, MAC address and machine ID. A container restored without its files shows why: **Retry setup** creates it from its image, downloading a catalog image when needed, and runs its compose setup again.

## Read job results

Longer operations appear under **Jobs** at the top right of every page. Its menu lists the jobs in progress, then the latest ones, each with its target, its stage or state and its percentage or start time; **All jobs** opens the **Jobs** page with every job the manager keeps. Open a job to see its status and output; the panel updates while work continues. A build, a snapshot, or a catalog image's download or build that failed or was interrupted, such as by a dropped connection to Docker, has **Retry**, which starts it again as it was asked, without filling in its form again. A build also has **Edit and retry**, which opens **Build image** with everything it was asked, so the script or any other field can change first. A container's creation has neither, since its secret values are kept nowhere else: a container that failed to start or set up has **Retry setup** of its own, and one that failed before it was saved is made again with **New container**. Image builds show script output and list their stages: preparing the base image, running the script, saving the image layer, writing and tagging the image and cleaning up. The current stage is highlighted with its running time, each finished one shows how long it took, and the job list names the current stage. Only a base image download has a progress bar; Docker reports no progress while the script runs or the layer is saved, so a large change can sit in **Saving the image layer** for minutes. Snapshots and settings changes that recreate a container list their stages too, and a large container layer can sit in **Committing the container layer** for minutes for the same reason. Image downloads show a progress bar with the downloaded size, and the job list shows their percentage. A container or an image that is still being made already appears under **Containers**, on the dashboard's table and under **Images**, with its name, how far its job is and **View log** for the job panel, but no controls until it is ready: a container until its desktop runs, a saved image until its record is written. A catalog image that is downloading or building shows its job the same way instead of its download and build buttons. Deletion jobs explain which container and networks were removed and confirm that named volumes and saved images were preserved, or name the volumes created for the container that were deleted with it.

You do not have to keep **Jobs** open to hear how a job ended. Every page, the desktop view included, follows the manager's [event stream](API.md#event-stream) and shows a short notice in the corner when a job fails or is interrupted, when one that makes something, such as a container, an image or a volume, completes, and when any other job completes after ten seconds or more, each with **View log**. Scripts and downloads, whose log or download is already in front of you, and agents' many small scripts stay quiet. A page in the background lets the stream go after a minute, so many open tabs do not use up the browser's connections; when you return, it reads everything again and tells which jobs ended meanwhile. The notices are part of the page, not system notifications, so they need neither HTTPS nor a permission.

If container creation failed partway through, **Retry setup** can complete missing setup without replacing an existing container. Agents can also reset a container through the API, which recreates the container from its saved image while keeping named volumes. See [recovery](INSTALLATION.md#when-setup-is-interrupted) for what can and cannot be restored.
