# Using Bilskirnir from an agent

[Back to the README](../README.md)

Bilskirnir includes an MCP server at `/mcp`, on the same port as the dashboard. An agent can create a container, install software, read and write files, inspect its screen, interact with applications and save the result as an image. You do not need a separate MCP container.

The server uses the [official Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk) and Streamable HTTP. The same operations are available through the [HTTP API](API.md) for scripts and other applications.

## Enable agent access

MCP is off by default. Generate a separate random token, for example with `openssl rand -hex 32`, and set these values in the manager's Compose environment:

```yaml
MCP_ENABLED: "true"
MCP_TOKEN: "your-random-token"
```

With the full installation manifests, set `MCP_ENABLED=true` and `MCP_TOKEN` in `.env` instead. Recreate the manager after changing its settings. The [WSL distribution package](WSL_PACKAGE.md) asks at first start whether to enable MCP and how agents authenticate; run `bilskirnir mcp` inside the distribution to see the endpoint together with the client settings below. An enabled server requires a token of 1 to 256 visible ASCII characters without spaces, even if dashboard login is disabled, unless `MCP_AUTH_MODE=none` is set. The length is your choice, but a short token is easy to guess.

Configure your MCP client with:

| Setting | Value |
| --- | --- |
| Transport | Streamable HTTP |
| URL | `http://localhost:8080/mcp` |
| Authorization header | `Bearer YOUR_MCP_TOKEN` |

Use the hostname and HTTPS address of your installation for LAN access. The token grants administrator access to the managed containers, including root script execution and deletion. It is separate from the dashboard password and does not expire with a browser session. Rotate it by changing `MCP_TOKEN` and recreating the manager.

`MCP_AUTH_MODE=none` accepts agent requests without a token; leave the authorization header out of the client settings, or keep it, since the manager then ignores it. The Host and Origin checks still apply, so a web page opened in a browser cannot call the endpoint, but any program that can reach the manager's address gets the same administrator access. Use it only while the manager listens on localhost.

Clients configure HTTP headers in different ways. Add the URL and bearer header using your client's MCP server settings; the server does not implement an OAuth sign-in flow. It uses stateless requests, so there is no session ID to save. The SDK handles protocol negotiation, including clients using the 2025 Streamable HTTP revisions.

## Connect through stdio on Windows

If your agent uses stdio, or something on Windows interferes with HTTP on localhost (see [broken responses from localhost](#broken-responses-from-localhost-on-windows)), use the bridge bundled in the manager image. It connects to the same `/mcp` service from inside the container and relays MCP messages over stdin/stdout.

For clients that use an `mcpServers` configuration:

```json
{
  "mcpServers": {
    "bilskirnir": {
      "command": "wsl.exe",
      "args": ["-d", "Bilskirnir", "--", "docker", "exec", "-i", "bilskirnir-manager", "python3", "/app/mcp_bridge.py"]
    }
  }
}
```

Use your WSL distribution and manager container names; `Bilskirnir` is the distribution installed from the package, and a manual installation uses its own distribution name such as `Ubuntu-24.04`. The local development preview uses `bilskirnir-preview`. For an agent already inside WSL, use `docker` as the command and start the argument list at `exec`.

MCP must still be enabled. The bridge uses the manager's existing `MCP_TOKEN`, or none with `MCP_AUTH_MODE=none`, so you do not put a token in the client command. Anyone able to run Docker exec already has access to that Docker Engine. The bridge negotiates MCP `2025-11-25`, which the connected client must support. HTTP clients can continue to use the normal endpoint directly.

### Broken responses from localhost on Windows

Some Windows security products scan HTTP traffic on localhost and rewrite responses on the way. The web protection in Norton does this for requests whose path is `/mcp` (and `GET /sse`), on any local server: it replaces a `Content-Length` header with `Transfer-Encoding: chunked` but leaves the body unchunked, so clients drop the connection with errors such as `Invalid character in chunk size` (Node.js) or `Malformed encoding found in chunked-encoding` (curl). A response that is already chunked passes through unchanged.

The manager therefore sends every `/mcp` response chunked, including the `401` for a missing token. Managers up to 0.2.0 still used `Content-Length` and are affected: update them (`bilskirnir update` in the WSL package), or exclude `localhost` or the manager's port from the product's web or HTTP scanning, or use the stdio bridge above, which reaches the manager without the Windows loopback connection.

To check what reaches Windows, send the same request from Windows and from inside the WSL distribution, where the security product does not see the traffic. In `token` mode both should print `401` with `transfer-encoding: chunked` and the JSON error body:

```bash
curl -si -X POST http://127.0.0.1:8080/mcp
```

## A useful first workflow

1. Call `get_settings` and `list_images` to see the available images and defaults.
2. Call `create_container` with a name and an image: a catalog image's `reference` or a saved image's `image_id`. A catalog image that is not downloaded yet is pulled first, which can take several minutes; `pull_image` fetches it ahead of time.
3. Poll `get_job` using the returned `job_id`. Once it completes, use the result's `container_id`.
4. Call `exec_container` to install applications or prepare files. Use `root=true` explicitly when package installation needs root.
5. Call `launch_application` to start a graphical program, then `act_and_observe` or `screenshot` and `desktop_input` to use it. Inspect the returned screenshot before the next step.
6. Call `snapshot` to save the desktop's filesystem changes. After that job completes, its image can start another container.
7. Delete a container only when its remaining container files are no longer needed. Saved images and named volumes are preserved.

An accepted job is not a completed operation. `get_job` reports `queued`, `running`, `completed`, `failed` or `interrupted`, with the log and result. While an image downloads, it also reports `progress` with `current` and `total` bytes and `layers_done` of `layers`. A `build_image` job lists its `stages` (`base`, `script`, `layer`, `image`, `cleanup`) with `started_at` and `finished_at` seconds, so a long build shows whether the script is still running or Docker is saving the image layer; only the base image download has a `progress`, as described in [the API guide](API.md). A `snapshot` job lists `check`, `commit` and `record`, and an `update_container` job that recreates the container lists `stop`, `commit`, `create` and `cleanup`, with `stop` skipped for a stopped container. Docker reports no progress while it commits the container layer, so `commit` only shows how long it has been running. Inspect a failed or interrupted job before trying again. A lost connection does not mean a queued operation was cancelled.

## Tools

The server currently exposes 75 tools:

| Work | Tools |
| --- | --- |
| Installation | `get_settings`, `get_system`, `get_usage` |
| Container lifecycle | `list_containers`, `get_container`, `create_container`, `update_container`, `container_action`, `set_internet`, `retry_container`, `reset_container`, `delete_container` |
| Work inside a container | `exec_container`, `read_file`, `write_file`, `upload_file`, `download_file`, `list_files`, `move_file`, `delete_file`, `screenshot`, `save_screenshot`, `desktop_input`, `act_and_observe`, `wait_for_condition`, `inspect_ui`, `gamepad_input`, `capture_audio`, `inject_audio`, `container_logs`, `container_diagnostics` |
| Interactive terminals | `open_terminal`, `list_terminals`, `terminal_input`, `terminal_output`, `close_terminal` |
| Applications and windows | `launch_application`, `list_applications`, `get_application`, `application_logs`, `application_diagnostics`, `sample_metrics`, `stop_application`, `list_windows`, `focus_window`, `set_window_geometry` |
| Recordings and evidence | `start_recording`, `stop_recording`, `list_recordings`, `export_artifacts` |
| Images | `list_images`, `get_image_layers`, `pull_image`, `build_catalog_image`, `import_image`, `build_image`, `get_image_compose`, `set_image_compose`, `snapshot`, `delete_image` |
| Persistent storage | `list_volumes`, `create_volume`, `copy_volume`, `delete_volume` |
| Large files and archives | `create_transfer` |
| MCP servers in desktops and endpoints | `list_mcp_servers`, `register_mcp_server`, `remove_mcp_server`, `list_mcp_endpoints`, `create_mcp_endpoint`, `update_mcp_endpoint`, `delete_mcp_endpoint`, `create_mcp_endpoint_token` |
| Operation results | `list_jobs`, `get_job` |

Each tool advertises its arguments and description through `tools/list`. Read operations are marked read-only; deletion, script execution, file replacement and desktop interaction carry destructive-operation hints for clients that show approvals.

Successful data results are returned as `structuredContent`, with text content for clients that do not use structured results. Lists are wrapped in an `items` object. `screenshot` and `act_and_observe` return a JPEG image content block with the image size in `structuredContent`. Tool failures return `isError=true` and an explanation; invalid tool names or missing/unknown top-level arguments return a protocol error.

## Run commands and install software

`exec_container` takes `container_id`, `script`, optional `root` and optional `timeout_seconds`. It runs bash with fail-on-error. The default user is UID/GID 1001:1001 and the working directory is `/home/desktop`.

For example:

```json
{
  "container_id": "CONTAINER_ID",
  "root": true,
  "timeout_seconds": 600,
  "script": "apt-get update\napt-get install -y --no-install-recommends git curl"
}
```

The container needs internet access for package downloads. Its CPU, memory and networking settings still apply. `root=true` is an administrator operation and does not depend on whether desktop-user sudo was enabled.

Output goes to the job log. A nonzero exit or timeout fails the job and records the exit status. The default time limit is 300 seconds; the allowed range is 1–3500. The script may contain at most 1,000,000 bytes. Scripts are copied into the desktop before they run, so large scripts do not rely on Docker command-line argument limits. The script's standard input is closed: a command that reads input receives end-of-file instead of consuming the rest of the script.

A script holds the container's operation lock until it finishes, so do not start a graphical application in the foreground of a script. Use [`launch_application`](#launch-and-observe-applications) instead; it returns immediately and leaves the desktop available for input and screenshots.

## Ask for values with composes

A compose makes a saved image ask for values, such as an account, when a container is created from it, and run a setup script with them once the desktop is ready. `set_image_compose` takes the record's `image_id` from `list_images` and a `compose`, or `null` to remove it; `get_image_compose` returns it with its script, and `list_images` shows the parameters without it. `build_image` accepts a `compose` for the image it builds, and a `description`, an `icon` (a PNG of at most 256 by 256 pixels as a `data:image/png;base64,` URL) and a `template_id`, so an exported build configuration or a template from the dashboard's **Templates** page is a request it takes as it is. Instead of one `script` it takes `parts`, the scripts of several templates merged into one image, each `{"id", "name", "script"}`, run in order and each in a layer of its own, with their ids in `template_ids`; such an image's compose has a step for each template, `{"steps": [...]}`, whose scripts receive only their own parameters, as [the HTTP API](API.md#merged-templates) describes. `list_images` names an image's icon by a version instead of carrying the picture.

```json
{
  "image_id": "IMAGE_RECORD_ID",
  "compose": {
    "parameters": [
      {"name": "UNITY_EMAIL", "label": "Unity email", "required": true},
      {"name": "UNITY_PASSWORD", "label": "Unity password", "required": true, "secret": true}
    ],
    "script": "set -eu\nprintf '%s\\n' \"$UNITY_EMAIL\" > ~/.unity-account\n"
  }
}
```

`create_container` then takes the values in `parameters`, such as `{"UNITY_EMAIL":"dev@example.com","UNITY_PASSWORD":"..."}`; with a compose of several steps a value is `STEP.NAME`, such as `"vscode.GIT_EMAIL"`, or the name alone when only one step asks for it. Its job completes after every script succeeded, and `get_container` reports the result in `setup`, with each step's `status` in `setup.steps`. A secret value is never returned and appears as `***` in the job log. A parameter's `type` is `text`, `number`, `boolean`, `choice`, `choices` or `volume`, and its `display` picks the dashboard control; the values an agent sends are text in the form the script reads, such as `"4"`, `"true"` or `"linux\nwebgl"`. A `text` parameter can have a `pattern`, a regular expression in the syntax of Rust's regex crate that a value must match as a whole, and a `pattern_message` describing the expected format, which the error repeats when a value does not match; an empty value is left to `required`. A `volume` parameter has a `path`, such as `/opt/unity`, and takes the name of a volume from `list_volumes`, other than one marked `manager`, or `create_volume`, or `""` for none: the volume is mounted at the path, and the script reads the path from the parameter's variable and the volume from the same name with `_VOLUME` appended. With `"new_volume": true`, `""` or no value gives the new container a volume of its own, `bilskirnir-CONTAINER_ID-name`, which its creation job creates, so there is no `create_volume` to call first and nothing to clean up when the request is refused; `"-"` gives it none instead, unless the parameter is required. `list_volumes` reports such a volume's `owner`: its `container_id`, `parameter` and the container's `name`, or `deleted: true` once that container was deleted. `delete_container` keeps it unless `delete_volumes` is `true`, and then removes only the volumes created for that container that no other container mounts, listing them in the job result as `volumes_deleted` and `volumes_kept`. The rules for names, values, volumes, reruns and snapshots are in [the HTTP API](API.md#composes).

## See what takes disk space

`list_images` gives every image Docker has three sizes in bytes: `size` as Docker reports it, `layers_size` unpacked and `own_size`, the part no other listed image brought. Images share the layers they have in common, so a snapshot's `size` includes its base desktop while its `own_size` is only what the container changed. `get_image_layers` takes an image's `id` from `list_images` and lists the steps of its history, newest first, each with the bytes it wrote, its `instruction` and `from`, the image it came from, so an agent can tell which step of a build script made an image large. The fields are described in [the HTTP API](API.md#image-sizes-and-layers).

`delete_image` takes an image's `id` from `list_images` and deletes it. It cannot be undone, so check the name first. A build, a snapshot or an archive loses its record and Docker deletes the image, unless another tag or another image still holds it; `image_removed` in the result tells which. An `import` was on the Docker host before it was registered, so only its record goes. A catalog image loses only its download and stays listed, so `pull_image` or a new container downloads it again. It answers at once, without a job, and deletes nothing when it refuses: for an image a container was created from, whose name the error gives, since `reset_container` creates the container from that image again, unless another saved image keeps the same Docker image, and for one Docker still uses elsewhere. Snapshots made from a deleted image keep working, because Docker keeps the layers they share with it.

## Read and write files

`read_file` takes a canonical absolute path and reads as the desktop user. Its default encoding is `utf8`; use `base64` for binary data. Each call reads up to 512 KiB, with a default of 64 KiB. Results include `bytes`, `size`, `next_offset` and `eof`; use `offset` and `limit` to page through larger files. Offsets count bytes. A UTF-8 slice that cuts a character or contains binary bytes needs a different boundary or base64 encoding.

`write_file` takes `path` and `data`, with the same encoding choices. It creates missing parent directories and saves the complete file atomically. Existing files require `replace=true`. Writes are limited to 512 KiB and use mode 0600. There is no root file-write option; destination permissions still apply.

Both tools reject path traversal and symlinks. Reads accept regular files only, so devices, sockets and FIFOs cannot turn a bounded read into an endless stream.

`list_files` returns a directory's entries with type, size, modification time and mode. `move_file` renames or moves within the same volume and needs `replace=true` to overwrite. `delete_file` removes a file, a symlink itself or, with `recursive=true`, a directory with its contents; it refuses top-level directories, the desktop home and mounted volume roots. `download_file` returns a whole regular file as base64 with its size and a guessed MIME type, up to 5 MiB; the HTTP download route streams larger files. Its result also carries `download_url`, described under [Hand files to the user](#hand-files-to-the-user).

`upload_file` is the smaller-file counterpart of the browser upload feature. It accepts standard base64 data and a destination folder anywhere in the desktop home, in a writable mounted volume or in one of the `places` of the container's `setup`. Its MCP limit is 512 KiB; for larger files, `create_transfer` issues a link to the streaming HTTP upload route that needs no dashboard session, as [move large files](#move-large-files) describes. See [files in the HTTP API](API.md#files).

## Launch and observe applications

`launch_application` starts a program in the background as the desktop user and returns an `application_id`. It takes `command`, optional `args`, `cwd` (default `/home/desktop`), `env` and `name`. Set `gpu=true` to run the program through `gpu-run` for hardware OpenGL; this needs a GPU-enabled container. A Chromium-based browser needs no flags of its own, as [running graphics applications](USAGE.md#running-graphics-applications) explains. The call returns after a short check, so a missing program or working directory is reported as `failed` immediately.

`get_application` and `list_applications` report `state` (`starting`, `running`, `exited`, `failed` or `unknown`), the process ID, `exit_code` or the terminating `signal`, and the log size. `application_logs` returns the last `tail_bytes` of the combined stdout/stderr log, which keeps about the latest 1 MiB. `stop_application` sends SIGTERM to the application's process group and SIGKILL after `grace_seconds`.

`application_diagnostics` adds the application's process tree with memory use, open file and thread counts and the latest log lines. `sample_metrics` measures CPU and resident memory of those processes over `duration_ms` and reports container memory from cgroups; a frame rate is not available, so judge rendering from screenshots.

`get_usage` reads the history the manager keeps of every running container, one sample a second for 30 minutes, and returns the latest, average and peak over the last `seconds` (default 60, at most 1800) for one `container_id` or for all of them. A deleted container is still reported, with `removed: true` and the name it had, until its samples are 30 minutes old. `cpu_cores` counts host CPU cores in use, not a share of the container's `cpu_limit`; disk and network are bytes per second. Its `gpu` is the whole host GPU from `nvidia-smi` while a GPU container runs: WSL cannot attribute GPU use to a container, so never read it as one container's share. See [resource usage](API.md#resource-usage) for the figures and their limits.

`wait_for_condition` replaces fixed delays: it polls until a window title appears, a launched application reaches `application_state` (default `exited`) or logs `log_contains`, or the screen (optionally a `region`) has not changed for `screen_stable_ms`. All supplied conditions must hold. After `timeout_ms` it returns `satisfied=false` with the last observation instead of an error.

Supervisor files live under `/tmp` in the desktop, so they are not part of snapshots and do not survive a desktop restart; afterwards the state is `unknown`. Application records are removed with their container. A launched application does not hold the container lock, so scripts, file access and desktop input keep working while it runs.

`list_windows` returns visible X11 windows with their IDs, titles, PIDs, geometry and the active window. `focus_window` raises a window so keyboard input reaches it, and `set_window_geometry` moves or resizes it so that screenshots and coordinates stay predictable. These tools, window screenshots and relative pointer movement use `xdotool` and `wmctrl` in the desktop image; desktops built from images that predate them need an updated image.

## Record and export evidence

`start_recording` captures the desktop as JPEG frames at `fps` (1–5) until `stop_recording`, `max_seconds` (up to 600), 3000 frames or 256 MiB. `max_width` scales frames (default 960) and `region` limits them to part of the desktop. `list_recordings` shows each recording's state, frame count and size; frames are regular files under `/tmp/bilskirnir-recordings/<recording_id>` and can be read with `read_file`. The frames are a sequence, not an encoded video.

Stopping a recording also writes `recording.avi`, an MJPEG video that VLC, mpv and ffmpeg play; the stop result describes it under `video`. `export_artifacts` packages recordings and application logs into a tar.gz with a `manifest.json`, saved in the desktop home, a writable mounted volume or a compose folder so the files outlive the desktop, and returns the archive's `path` and `download_url`. Recording and application files live in tmpfs and disappear when the desktop restarts, so export them before resetting or restarting a container.

## Listen, talk, type and press buttons

`capture_audio` records what the desktop plays for up to 10 seconds and returns a WAV audio block plus `peak`, `rms` and `silent`, so an agent can tell whether sound is produced even without listening. With `source=microphone` it records what reaches the desktop's microphone. `inject_audio` plays 16-bit PCM WAV data into the microphone or the speakers to test voice input or playback. Both need audio enabled for the container; the microphone side also needs the microphone option.

`open_terminal` starts bash on a pseudo-terminal and returns a `terminal_id`; `terminal_input` types text (with Enter by default) and `terminal_output` reads from a byte offset, returning `next_offset` for incremental reads and stripping escape sequences. Use it for installers that ask questions, debuggers and other interactive programs; `close_terminal` hangs up. `root=true` opens a root shell and is an explicit administrator choice. Terminal state lives in tmpfs and ends with a desktop restart.

`inspect_ui` reads the AT-SPI accessibility tree of applications that expose one, such as GTK and Electron applications or Qt with `QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1`. Nodes carry roles, names, states, screen extents, text and action names, which lets an agent find a button by name instead of guessing pixels. Games and Unity players usually expose no tree; use screenshots for those.

`gamepad_input` drives a virtual Xbox-style controller in containers created with `gamepad=true`. Buttons stay pressed and axes keep their value between requests until `reset`, so holding a stick while pressing a button is a two-action request. The device appears as `/dev/input/eventN` inside the desktop; SDL applications normally detect it, and `SDL_JOYSTICK_DISABLE_UDEV=1` in `launch_application` env helps those that rely on udev.

## Use the mouse and keyboard

`desktop_input` sends actions through RFB, the VNC protocol, to the same desktop used by the browser. It opens a shared connection, so another viewer can stay connected. Coordinates are desktop pixels; they match a screenshot taken at full resolution, not a scaled or cropped image.

```json
{
  "container_id": "CONTAINER_ID",
  "actions": [
    {"type": "click", "x": 400, "y": 250},
    {"type": "key_press", "keys": ["Control", "a"]},
    {"type": "type_text", "text": "Hello from the agent"},
    {"type": "key_press", "keys": ["Enter"]}
  ]
}
```

Supported actions are `move`, `move_relative`, `click`, `button_down`, `button_up`, `drag`, `scroll`, `key_press`, `key_down`, `key_up`, `hold_keys`, `type_text`, `wait` and `release_all`. Clicks support left/middle/right buttons and up to three clicks. Drags have start/end coordinates and a duration of 100–2000 ms. Scroll accepts up/down/left/right and 1–50 steps.

Key chords press the supplied keys together, then release them in reverse order; a chord has 1–8 distinct keys. Special names include Control, Alt, Shift, Super, Enter, Tab, Escape, Backspace, Delete, Insert, Home, End, PageUp, PageDown, Space, arrows and F1–F12. Text supports Unicode; a newline presses Enter and a tab presses Tab.

`key_down` and `button_down` keep keys or buttons pressed for the following actions in the same request, so an agent can walk with W held while turning with the mouse, or drag with a held button. `hold_keys` presses keys for a given time. Everything still held is released when the request ends, and the response lists it under `released`; use `key_up`, `button_up` or `release_all` to release earlier. Held input does not carry over to the next request.

`move_relative` moves the pointer by `dx`,`dy` from its current position, optionally over several `steps` and a `duration_ms`. The manager emulates it with absolute positions, starting from the pointer location reported by the desktop. Applications that grab the pointer and read raw motion, such as first-person cameras, may not respond like they would to a physical mouse; verify the application before relying on it.

Requests allow 1–100 actions, up to 10,000 typed characters, single waits or holds of at most 10 seconds and at most 30 seconds of combined waits, holds and gesture delays. The whole action batch is validated against the connected desktop's dimensions before input is sent.

`screenshot` captures the desktop at full resolution by default. `max_width` scales the image down, `region` crops `x,y,width,height` and `window_id` captures one window from `list_windows`. The result's `structuredContent` holds `width`, `height` and `captured_at`. `act_and_observe` sends actions, waits `settle_ms` (default 500) and returns the screenshot together with the input result, which is the shortest loop for driving an application.

`save_screenshot` takes the same options under `capture` and writes the JPEG inside the desktop at `path`, an absolute file path in the desktop home, a writable mounted volume or a compose folder, as the desktop user and under the upload destination rules. It returns the file's `path`, `bytes`, `width`, `height`, `captured_at` and `download_url`. An existing file needs `replace=true`. `screenshot` stays a read-only tool; the image it returns reaches the agent, not necessarily the person.

## Hand files to the user

Tool results reach the agent, not necessarily the person reading the conversation: many clients never show an image returned by a tool, and relaying a file as base64 through the model is expensive and bounded. The results of `save_screenshot`, `download_file` and `export_artifacts` therefore include `download_url`, a link such as `/api/containers/ID/files/download?path=/home/desktop/Downloads/clock.jpg`. It is relative to the manager's address, the same host and port that serves `/mcp`, so it is valid wherever the manager is reachable: from a browser on the same machine, over the LAN with the HTTPS address, or from another operating system. The link opens in the dashboard session and streams the file with the limits of the HTTP download route; the MCP token does not authorize it, and an agent that fetches a file for itself uses `create_transfer` instead. Share the link, or the saved path, instead of looking for `docker`, `wsl` or a host shell: the server instructions tell agents that containers are reachable only through the tools and the HTTP API, because the client may run on a different machine than the manager.

The bundled TigerVNC server acknowledges input with RFB Fence. This confirms that the server processed the events; an application may still need time to update. Inspect the next screenshot before making a decision. If a connection fails after input starts, check the screen before repeating an action that could submit, purchase or delete something. A custom desktop VNC server must support RFB 3.8, shared connections and Fence.

## Move large files

The file tools carry at most 512 KiB per call as base64 through the conversation, and the HTTP routes that stream files and archives of any size take the dashboard's session cookie, not the MCP token. `create_transfer` connects the two: it issues a link for one HTTP request, which the agent sends with curl or another HTTP client from the machine it runs on.

| `operation` | Arguments | Request |
| --- | --- | --- |
| `container_upload` | `container_id`, `destination`, `name`, optional `replace` | `PUT` or `POST` the file as the body |
| `container_download` | `container_id`, `path` | `GET` the file |
| `volume_upload` | `volume`, `destination`, `name`, optional `replace` | `PUT` or `POST` the file |
| `volume_download` | `volume`, `path` | `GET` the file |
| `volume_import` | `name` of a new volume | `PUT` or `POST` a `.tar.gz` or `.tar` |
| `volume_extract` | `volume`, `path` of a new folder in it | `PUT` or `POST` a `.tar.gz` or `.tar` |
| `volume_export` | `volume` | `GET` a `.tar.gz` |
| `image_import` | optional `name` and `description` | `PUT` or `POST` an image archive |
| `image_export` | `image_id` | `GET` an image archive |

The result holds `url`, relative to the manager's address like `download_url`, the `method`, `expires_in_seconds` and `expires_at`:

```bash
curl --fail-with-body --upload-file project.tar.gz http://bilskirnir.lan:8080/api/transfers/TOKEN
curl --fail --output build.zip http://bilskirnir.lan:8080/api/transfers/TOKEN
```

The request runs the HTTP route the dashboard uses, with its limits, locks, job and progress, and answers as that route does: an upload with the saved path, an import with a `job_id` once the whole archive has arrived, an export with the archive and its job in the `X-Job-Id` header. Follow jobs with `get_job`. An upload's size comes from `Content-Length`, which `--upload-file` sends for a file, or from a `size` query parameter for a body sent without one. An import also takes an archive of unknown size, such as `tar -c -C project . | curl --upload-file - URL`, with the checks the [HTTP API](API.md#volume-archives) describes. Nothing else in the request changes what the link does.

A link is a credential of its own:

- Its token is random, 244 bits, kept in the manager's memory only, so a restart forgets it.
- It expires after five minutes, and the first request that presents it spends it, whatever that request's outcome. A transfer that has begun runs to its end; signing out of the dashboard does not stop it.
- It runs only the operation and arguments it was issued for, and it is neither a session cookie nor a bearer token for any other route.
- It needs no cookie, no `X-Bilskirnir-Request` header and no `Authorization` header, so a shell can use it without the administrator password or the MCP token. If the request carries an `Origin` header, it must match the manager's address.
- The manager keeps the token out of job logs, error messages and its request tracing. Treat the URL as a secret until it has been used: whoever holds it can run that one request.

Links come only from `create_transfer`, so with MCP disabled the route answers `404`. Accepting the MCP token on the HTTP API itself would have been simpler, but it would put a long-lived administrator credential into curl commands and shell history, where a leak reaches every route for as long as the token is configured.

## MCP servers inside a desktop

A desktop can run MCP servers of its own: a browser automation server attached to its Chrome, an editor's server, or one you write for a project, such as a Chrome extension you are developing. Each is declared in a JSON file named after it, in one of two folders:

| Folder | Who writes it | Source |
| --- | --- | --- |
| `/etc/bilskirnir/mcp.d/NAME.json` | The image: a template's or build's `mcp` declarations, or its script. | `image` |
| `~/.config/bilskirnir/mcp/NAME.json` | The desktop user, at any time: `bilskirnir-mcp add`, the dashboard's MCP page or `register_mcp_server`. | `container` |

The desktop user's declaration of a name replaces the image's. A declaration is either a program that speaks MCP on stdio or a Streamable HTTP server already listening on the desktop's loopback:

```json
{"description": "Playwright attached to the desktop's Chrome",
 "command": "playwright-mcp", "args": ["--cdp-endpoint", "http://127.0.0.1:9223"],
 "env": {"PLAYWRIGHT_BROWSERS_PATH": "0"}, "cwd": "/home/desktop"}
```

```json
{"url": "http://127.0.0.1:8931/mcp", "headers": {"Authorization": "Bearer ${API_TOKEN}"}}
```

Values in `args`, `env`, `url` and `headers` may name the container's [compose](API.md#composes) values as `${NAME}`, or `${STEP.NAME}` for one step of several, secret ones included; the manager fills them in when it connects, so a secret never has to be written into the image or the file. Inside the desktop, `bilskirnir-mcp` writes the desktop user's files:

```bash
bilskirnir-mcp add x --description "My extension's helper" -- node /home/desktop/ext/mcp.js
bilskirnir-mcp add api --url http://127.0.0.1:8931/mcp --header 'Authorization: Bearer ${API_TOKEN}'
bilskirnir-mcp list
bilskirnir-mcp remove x
```

The manager watches both folders in every running desktop. A new or changed server is started once to read its tool list, which is kept, so its tools stay listed while the container is stopped. `list_mcp_servers` reports each server's `state`: `ready`, `pending` while its tools are read, `error` with the reason, or `stopped` while the container does not run, with the end of its stderr in `log`. A server that sends `notifications/tools/list_changed` has its list read again.

A server is never published on a port and never reached over the desktop's network. The manager runs it with `docker exec` as the desktop user, with `DISPLAY`, `HOME` and a `PATH` that includes `~/.local/bin`: a stdio program directly, an HTTP server through a small adapter inside the desktop that connects to its loopback address, so a server that listens on `127.0.0.1` only works, and its own `Authorization` header is sent. Root is never used. A server whose declaration the image carries cannot be removed from a container, only left out of an endpoint.

## MCP endpoints for one agent

`/mcp` is the administrator's channel: whoever holds its token can create, change and delete containers and run scripts as root. An agent that should only drive a desktop's browser, or a project's own MCP server, gets an endpoint instead, at `/mcp/NAME` on the same address. An endpoint covers one or several containers, and for each the scopes and desktop servers chosen for it:

| Scope | Tools, bound to that container |
| --- | --- |
| `view` | `get_container`, `container_logs`, `container_diagnostics`, `get_usage`, `screenshot`, `save_screenshot`, `list_windows`, `inspect_ui`, `wait_for_condition` |
| `input` | `desktop_input`, `act_and_observe`, `focus_window`, `set_window_geometry`, `gamepad_input` |
| `applications` | `launch_application`, `list_applications`, `get_application`, `application_logs`, `stop_application`, `application_diagnostics`, `sample_metrics` |
| `files_read` | `list_files`, `read_file`, `download_file` |
| `files_write` | `write_file`, `upload_file`, `move_file`, `delete_file` |
| `commands` | `exec_container` and the terminal tools, as the desktop user; with `sudo` enabled for the container that is root inside it |
| `media` | `start_recording`, `stop_recording`, `list_recordings`, `export_artifacts`, `capture_audio`, `inject_audio` |
| `power` | `container_action` |

`get_job` comes with `commands` or `power`, for the jobs of the endpoint's own containers. No scope creates, changes, resets, snapshots or deletes a container, and none reaches images, volumes, other containers or root. The desktop servers are `all`, including those declared later, or a list of names.

Tools keep their names and lose `container_id`: an endpoint of one container is bound to it, and on an endpoint of several each tool takes `container`, one of the aliases of the containers granted its scope. A desktop server's tools are named `SERVER__TOOL`, or `ALIAS__SERVER__TOOL` on an endpoint of several containers, at most 64 characters, and their descriptions start with the container and server. Their annotations, such as `readOnlyHint`, come from inside the desktop and are left out, so a client asks before running them as it would for any tool. When a server is added, removed or changes its tools, the endpoint sends `notifications/tools/list_changed`: on the stream a session of protocol `2025-11-25` or earlier opens, and on `subscriptions/listen` for `2026-07-28`. Agents on one endpoint share each server's connection, and so its state, such as a browser's pages; another endpoint has its own. A connection unused for 10 minutes is closed.

Every new container gets an endpoint of its own unless `create_container` is given `"mcp_endpoint": false`: named after the container, with every scope but `commands`, all its desktop servers and no token yet. Deleting the container deletes it. Other endpoints are made with `create_mcp_endpoint`, or on the dashboard's MCP page, and outlive their containers: a deleted container only leaves their grants.

An endpoint opens with its own bearer token or the administrator's MCP token. `create_mcp_endpoint` and `create_mcp_endpoint_token` return the token once; the manager keeps only its hash, and a new token ends the old one and its sessions. The endpoint's token opens nothing else: not `/mcp`, not the HTTP API, not the desktop ports. The endpoints follow `MCP_ENABLED` and the Host and Origin checks of `/mcp`, and with `MCP_AUTH_MODE=none` they need no token either. A client connects to an endpoint as to `/mcp`:

```bash
claude mcp add --transport http chrome-dev http://localhost:8080/mcp/chrome-dev --header "Authorization: Bearer ENDPOINT_TOKEN"
```

## Reach a server inside the desktop

For an MCP server, prefer a declaration and an endpoint, described above: they need no port and work with servers that listen on `127.0.0.1` only. A program in the desktop may serve its own API: an MCP server of the Unity editor, a test runner endpoint, a debug server. Its ports are not published, but the manager forwards HTTP and WebSocket requests to the ports a container's `ports` setting lists, at `/api/containers/CONTAINER_ID/ports/PORT/` followed by the service's own path. Set `ports` with `create_container` or `update_container`, such as `"ports": [8090]`; it is empty by default, and a change applies without restarting the desktop. The server must listen on the desktop's network, such as on `0.0.0.0`, not on `127.0.0.1` only, which the manager cannot reach.

The route takes the same bearer token as `/mcp`. An MCP server listening on port 8090 at `/mcp` inside the desktop is therefore a second MCP server for a remote agent:

| Setting | Value |
| --- | --- |
| Transport | Streamable HTTP |
| URL | `http://localhost:8080/api/containers/CONTAINER_ID/ports/8090/mcp` |
| Authorization header | `Bearer YOUR_MCP_TOKEN` |

Responses stream as the server sends them, so server-sent events work, and a WebSocket handshake joins the client to the service. The service sees a local client: `Host` is `localhost:PORT`, and the token, the dashboard cookie and `Origin` are removed, so a service that requires its own `Authorization` header cannot be reached this way. Treat what it answers as container data, like screenshots and command output. A browser cannot use the route with the dashboard session, and every response is sandboxed, so a page the desktop serves runs no script on the dashboard's address; [the API guide](API.md#desktop-ports) has the details.

## Keep agent work recoverable

MCP uses the same ownership checks, container locks, jobs and snapshot behavior as the dashboard. An operation on another container can proceed independently. A script running in one container holds that container's operation lock until it finishes; a launched application does not.

Read/write and execution tools operate inside managed desktop containers. They do not expose host shell execution or give desktop containers the host Docker socket. Treat files, logs and screen contents as data from the container, especially when the desktop is browsing external sites.

`reset_container` gives a test a clean starting point: it discards the container's unsaved changes and recreates it from its saved source image, keeping the container ID, networks, named volumes and [machine identity](API.md#machine-identity): the hostname, MAC address and machine ID that a license bound to one machine sees. A pool of desktops reset between tasks therefore stays the same set of machines, and a new container, also one from a snapshot, is a new machine unless `create_container` is given the `hostname`, `mac_address` and `machine_id` of the container it replaces. Data inside mounted volumes is not touched; clean it explicitly with `delete_file` or a script when a test needs an empty volume. Take a snapshot first if the current state should be reusable.

`update_container` changes a container's settings after creation; pass only the fields to change. `name`, `internet`, `ports` and a new or changed resource limit apply in place. Use `dry_run=true` first: its `restart` list names the changes that recreate the container, which restarts the desktop and closes running applications while keeping the files in its container layer.

Snapshots save filesystem changes; they do not save volumes or running application state. Use a named volume for project files that must survive container deletion, and save an image before deleting a container whose installation you want to reuse. `list_volumes` marks the manager's own volumes, such as its data volume, with `manager: true`; a container cannot mount them, so pick another one. It lists only the volumes that carry the installation's `VOLUME_FILTER` label, by default the ones Bilskirnir created; a volume it leaves out cannot be mounted or opened, so create or copy one instead of naming a volume from elsewhere on the host.

`copy_volume` copies a named volume, `volume`, into a new one called `name`: a prepared project or cache that each task starts from, or a copy kept before a risky change. Poll `get_job` until it completes before mounting the copy. The copy keeps the tree and modes, and its files belong to the desktop user. It is a full copy, not copy-on-write, so it takes as much disk space as the volume. The rules are in [volume archives](API.md#volume-archives).

`delete_volume` deletes a named volume, `name`, with its files, such as one `list_volumes` shows with `owner.deleted: true` because its container was deleted without it. It cannot be undone, so check the name first. It answers at once, without a job, and deletes nothing when it refuses: for the manager's own volumes, for a volume a managed container mounts through its volumes or its compose, whose name the error gives, and for one another Docker container still uses. A volume `VOLUME_FILTER` leaves out is answered as missing, as `list_volumes` leaves it out.
