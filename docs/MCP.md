# Using Bilskirnir from an agent

[Back to the README](../README.md)

Bilskirnir includes an MCP server at `/mcp`, on the same port as the dashboard. An agent can create a desktop, install software, read and write files, inspect its screen, interact with applications and save the result as an image. You do not need a separate MCP container.

The server uses the [official Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk) and Streamable HTTP. The same operations are available through the [HTTP API](API.md) for scripts and other applications.

## Enable agent access

MCP is off by default. Generate a separate random token, for example with `openssl rand -hex 32`, and set these values in the manager's Compose environment:

```yaml
MCP_ENABLED: "true"
MCP_TOKEN: "your-random-token"
```

With the full installation manifests, set `MCP_ENABLED=true` and `MCP_TOKEN` in `.env` instead. Recreate the manager after changing its settings. The [WSL distribution package](WSL_PACKAGE.md) enables MCP at first start with a generated token; run `bilskirnir mcp` inside the distribution to see it together with the client settings below. An enabled server requires a token of 32–256 visible ASCII characters, even if dashboard login is disabled.

Configure your MCP client with:

| Setting | Value |
| --- | --- |
| Transport | Streamable HTTP |
| URL | `http://localhost:8080/mcp` |
| Authorization header | `Bearer YOUR_MCP_TOKEN` |

Use the hostname and HTTPS address of your installation for LAN access. The token grants administrator access to the managed desktops, including root script execution and deletion. It is separate from the dashboard password and does not expire with a browser session. Rotate it by changing `MCP_TOKEN` and recreating the manager.

Clients configure HTTP headers in different ways. Add the URL and bearer header using your client's MCP server settings; the server does not implement an OAuth sign-in flow. It uses stateless requests, so there is no session ID to save. The SDK handles protocol negotiation, including clients using the 2025 Streamable HTTP revisions.

## Connect through stdio on Windows

If your agent uses stdio, or your Windows localhost forwarder interferes with HTTP streaming, use the bridge bundled in the manager image. It connects to the same `/mcp` service from inside the container and relays MCP messages over stdin/stdout.

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

MCP must still be enabled. The bridge uses the manager's existing `MCP_TOKEN`, so you do not put a token in the client command. Anyone able to run Docker exec already has access to that Docker Engine. The bridge negotiates MCP `2025-11-25`, which the connected client must support. HTTP clients can continue to use the normal endpoint directly.

## A useful first workflow

1. Call `get_settings` and `list_profiles` to see the available desktops and defaults.
2. Call `create_environment` with a name and an available image reference.
3. Poll `get_job` using the returned `job_id`. Once it completes, use the result's `environment_id`.
4. Call `exec_environment` to install applications or prepare files. Use `root=true` explicitly when package installation needs root.
5. Call `launch_application` to start a graphical program, then `act_and_observe` or `screenshot` and `desktop_input` to use it. Inspect the returned screenshot before the next step.
6. Call `snapshot` to save the desktop's filesystem changes. After that job completes, its image can start another environment.
7. Delete an environment only when its remaining container files are no longer needed. Saved images and named volumes are preserved.

An accepted job is not a completed operation. `get_job` reports `queued`, `running`, `completed`, `failed` or `interrupted`, with the log and result. Inspect a failed or interrupted job before trying again. A lost connection does not mean a queued operation was cancelled.

## Tools

The server currently exposes 57 tools:

| Work | Tools |
| --- | --- |
| Installation and profiles | `get_settings`, `get_system`, `list_profiles`, `build_profile` |
| Desktop lifecycle | `list_environments`, `get_environment`, `create_environment`, `environment_action`, `set_internet`, `retry_environment`, `reset_environment`, `delete_environment` |
| Work inside a desktop | `exec_environment`, `read_file`, `write_file`, `upload_file`, `download_file`, `list_files`, `move_file`, `delete_file`, `screenshot`, `desktop_input`, `act_and_observe`, `wait_for_condition`, `inspect_ui`, `gamepad_input`, `capture_audio`, `inject_audio`, `environment_logs`, `environment_diagnostics` |
| Interactive terminals | `open_terminal`, `list_terminals`, `terminal_input`, `terminal_output`, `close_terminal` |
| Applications and windows | `launch_application`, `list_applications`, `get_application`, `application_logs`, `application_diagnostics`, `sample_metrics`, `stop_application`, `list_windows`, `focus_window`, `set_window_geometry` |
| Recordings and evidence | `start_recording`, `stop_recording`, `list_recordings`, `export_artifacts` |
| Reusable images | `list_images`, `import_image`, `build_image`, `snapshot` |
| Persistent storage | `list_volumes`, `create_volume` |
| Operation results | `list_jobs`, `get_job` |

Each tool advertises its arguments and description through `tools/list`. Read operations are marked read-only; deletion, script execution, file replacement and desktop interaction carry destructive-operation hints for clients that show approvals.

Successful data results are returned as `structuredContent`, with text content for clients that do not use structured results. Lists are wrapped in an `items` object. `screenshot` and `act_and_observe` return a JPEG image content block with the image size in `structuredContent`. Tool failures return `isError=true` and an explanation; invalid tool names or missing/unknown top-level arguments return a protocol error.

## Run commands and install software

`exec_environment` takes `environment_id`, `script`, optional `root` and optional `timeout_seconds`. It runs bash with fail-on-error. The default user is UID/GID 1001:1001 and the working directory is `/home/desktop`.

For example:

```json
{
  "environment_id": "ENVIRONMENT_ID",
  "root": true,
  "timeout_seconds": 600,
  "script": "apt-get update\napt-get install -y --no-install-recommends git curl"
}
```

The environment needs internet access for package downloads. Its CPU, memory and networking settings still apply. `root=true` is an administrator operation and does not depend on whether desktop-user sudo was enabled.

Output goes to the job log. A nonzero exit or timeout fails the job and records the exit status. The default time limit is 300 seconds; the allowed range is 1–3500. The script may contain at most 1,000,000 bytes. Scripts are copied into the desktop before they run, so large scripts do not rely on Docker command-line argument limits. The script's standard input is closed: a command that reads input receives end-of-file instead of consuming the rest of the script.

A script holds the environment's operation lock until it finishes, so do not start a graphical application in the foreground of a script. Use [`launch_application`](#launch-and-observe-applications) instead; it returns immediately and leaves the desktop available for input and screenshots.

## Read and write files

`read_file` takes a canonical absolute path and reads as the desktop user. Its default encoding is `utf8`; use `base64` for binary data. Each call reads up to 512 KiB, with a default of 64 KiB. Results include `bytes`, `size`, `next_offset` and `eof`; use `offset` and `limit` to page through larger files. Offsets count bytes. A UTF-8 slice that cuts a character or contains binary bytes needs a different boundary or base64 encoding.

`write_file` takes `path` and `data`, with the same encoding choices. It creates missing parent directories and saves the complete file atomically. Existing files require `replace=true`. Writes are limited to 512 KiB and use mode 0600. There is no root file-write option; destination permissions still apply.

Both tools reject path traversal and symlinks. Reads accept regular files only, so devices, sockets and FIFOs cannot turn a bounded read into an endless stream.

`list_files` returns a directory's entries with type, size, modification time and mode. `move_file` renames or moves within the same volume and needs `replace=true` to overwrite. `delete_file` removes a file, a symlink itself or, with `recursive=true`, a directory with its contents; it refuses top-level directories, the desktop home and mounted volume roots. `download_file` returns a whole regular file as base64 with its size and a guessed MIME type, up to 5 MiB; the HTTP download route streams larger files.

`upload_file` is the smaller-file counterpart of the browser upload feature. It accepts standard base64 data and a destination within Downloads or a writable mounted volume. Its MCP limit is 512 KiB; use the streaming HTTP upload endpoint for larger files. See [files in the HTTP API](API.md#files).

## Launch and observe applications

`launch_application` starts a program in the background as the desktop user and returns an `application_id`. It takes `command`, optional `args`, `cwd` (default `/home/desktop`), `env` and `name`. Set `gpu=true` to run the program through `gpu-run` for hardware OpenGL; this needs a GPU-enabled environment. The call returns after a short check, so a missing program or working directory is reported as `failed` immediately.

`get_application` and `list_applications` report `state` (`starting`, `running`, `exited`, `failed` or `unknown`), the process ID, `exit_code` or the terminating `signal`, and the log size. `application_logs` returns the last `tail_bytes` of the combined stdout/stderr log, which keeps about the latest 1 MiB. `stop_application` sends SIGTERM to the application's process group and SIGKILL after `grace_seconds`.

`application_diagnostics` adds the application's process tree with memory use, open file and thread counts and the latest log lines. `sample_metrics` measures CPU and resident memory of those processes over `duration_ms` and reports container memory from cgroups; a frame rate is not available, so judge rendering from screenshots.

`wait_for_condition` replaces fixed delays: it polls until a window title appears, a launched application reaches `application_state` (default `exited`) or logs `log_contains`, or the screen (optionally a `region`) has not changed for `screen_stable_ms`. All supplied conditions must hold. After `timeout_ms` it returns `satisfied=false` with the last observation instead of an error.

Supervisor files live under `/tmp` in the desktop, so they are not part of snapshots and do not survive a desktop restart; afterwards the state is `unknown`. Application records are removed with their environment. A launched application does not hold the environment lock, so scripts, file access and desktop input keep working while it runs.

`list_windows` returns visible X11 windows with their IDs, titles, PIDs, geometry and the active window. `focus_window` raises a window so keyboard input reaches it, and `set_window_geometry` moves or resizes it so that screenshots and coordinates stay predictable. These tools, window screenshots and relative pointer movement use `xdotool` and `wmctrl` in the desktop image; desktops built from images that predate them need an updated image.

## Record and export evidence

`start_recording` captures the desktop as JPEG frames at `fps` (1–5) until `stop_recording`, `max_seconds` (up to 600), 3000 frames or 256 MiB. `max_width` scales frames (default 960) and `region` limits them to part of the desktop. `list_recordings` shows each recording's state, frame count and size; frames are regular files under `/tmp/bilskirnir-recordings/<recording_id>` and can be read with `read_file`. The frames are a sequence, not an encoded video.

Stopping a recording also writes `recording.avi`, an MJPEG video that VLC, mpv and ffmpeg play; the stop result describes it under `video`. `export_artifacts` packages recordings and application logs into a tar.gz with a `manifest.json`, saved in Downloads or a writable mounted volume so the files outlive the desktop or can be collected from the host. Recording and application files live in tmpfs and disappear when the desktop restarts, so export them before resetting or restarting an environment.

## Listen, talk, type and press buttons

`capture_audio` records what the desktop plays for up to 10 seconds and returns a WAV audio block plus `peak`, `rms` and `silent`, so an agent can tell whether sound is produced even without listening. With `source=microphone` it records what reaches the desktop's microphone. `inject_audio` plays 16-bit PCM WAV data into the microphone or the speakers to test voice input or playback. Both need audio enabled for the environment; the microphone side also needs the microphone option.

`open_terminal` starts bash on a pseudo-terminal and returns a `terminal_id`; `terminal_input` types text (with Enter by default) and `terminal_output` reads from a byte offset, returning `next_offset` for incremental reads and stripping escape sequences. Use it for installers that ask questions, debuggers and other interactive programs; `close_terminal` hangs up. `root=true` opens a root shell and is an explicit administrator choice. Terminal state lives in tmpfs and ends with a desktop restart.

`inspect_ui` reads the AT-SPI accessibility tree of applications that expose one, such as GTK and Electron applications or Qt with `QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1`. Nodes carry roles, names, states, screen extents, text and action names, which lets an agent find a button by name instead of guessing pixels. Games and Unity players usually expose no tree; use screenshots for those.

`gamepad_input` drives a virtual Xbox-style controller in environments created with `gamepad=true`. Buttons stay pressed and axes keep their value between requests until `reset`, so holding a stick while pressing a button is a two-action request. The device appears as `/dev/input/eventN` inside the desktop; SDL applications normally detect it, and `SDL_JOYSTICK_DISABLE_UDEV=1` in `launch_application` env helps those that rely on udev.

## Use the mouse and keyboard

`desktop_input` sends actions through RFB, the VNC protocol, to the same desktop used by the browser. It opens a shared connection, so another viewer can stay connected. Coordinates are desktop pixels; they match a screenshot taken at full resolution, not a scaled or cropped image.

```json
{
  "environment_id": "ENVIRONMENT_ID",
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

The bundled TigerVNC server acknowledges input with RFB Fence. This confirms that the server processed the events; an application may still need time to update. Inspect the next screenshot before making a decision. If a connection fails after input starts, check the screen before repeating an action that could submit, purchase or delete something. A custom desktop VNC server must support RFB 3.8, shared connections and Fence.

## Keep agent work recoverable

MCP uses the same ownership checks, environment locks, jobs and snapshot behavior as the dashboard. An operation on another desktop can proceed independently. A script running in one environment holds that environment's operation lock until it finishes; a launched application does not.

Read/write and execution tools operate inside managed desktop containers. They do not expose host shell execution or give desktop containers the host Docker socket. Treat files, logs and screen contents as data from the environment, especially when the desktop is browsing external sites.

`reset_environment` gives a test a clean starting point: it discards the container's unsaved changes and recreates it from the environment's saved source image, keeping the environment ID, networks and named volumes. Data inside mounted volumes is not touched; clean it explicitly with `delete_file` or a script when a test needs an empty volume. Take a snapshot first if the current state should be reusable.

Snapshots save filesystem changes; they do not save volumes or running application state. Use a named volume for project files that must survive environment deletion, and save an image before deleting a desktop whose installation you want to reuse.
