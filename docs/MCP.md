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

With the full installation manifests, set `MCP_ENABLED=true` and `MCP_TOKEN` in `.env` instead. Recreate the manager after changing its settings. An enabled server requires a token of 32–256 visible ASCII characters, even if dashboard login is disabled.

Configure your MCP client with:

| Setting | Value |
| --- | --- |
| Transport | Streamable HTTP |
| URL | `http://localhost:8080/mcp` |
| Authorization header | `Bearer YOUR_MCP_TOKEN` |

Use the hostname and HTTPS address of your installation for LAN access. The token grants administrator access to the managed desktops, including root script execution and deletion. It is separate from the dashboard password and does not expire with a browser session. Rotate it by changing `MCP_TOKEN` and recreating the manager.

Clients configure HTTP headers in different ways. Add the URL and bearer header using your client's MCP server settings; the server does not implement an OAuth sign-in flow. It uses stateless requests, so there is no session ID to save. The SDK handles protocol negotiation, including clients using the 2025 Streamable HTTP revisions.

## A useful first workflow

1. Call `get_settings` and `list_profiles` to see the available desktops and defaults.
2. Call `create_environment` with a name and an available image reference.
3. Poll `get_job` using the returned `job_id`. Once it completes, use the result's `environment_id`.
4. Call `exec_environment` to install applications or prepare files. Use `root=true` explicitly when package installation needs root.
5. Call `screenshot`, then `desktop_input` to use a graphical application. Take another screenshot to inspect the outcome.
6. Call `snapshot` to save the desktop's filesystem changes. After that job completes, its image can start another environment.
7. Delete an environment only when its remaining container files are no longer needed. Saved images and named volumes are preserved.

An accepted job is not a completed operation. `get_job` reports `queued`, `running`, `completed`, `failed` or `interrupted`, with the log and result. Inspect a failed or interrupted job before trying again. A lost connection does not mean a queued operation was cancelled.

## Tools

The server currently exposes 27 tools:

| Work | Tools |
| --- | --- |
| Installation and profiles | `get_settings`, `get_system`, `list_profiles`, `build_profile` |
| Desktop lifecycle | `list_environments`, `get_environment`, `create_environment`, `environment_action`, `set_internet`, `retry_environment`, `delete_environment` |
| Work inside a desktop | `exec_environment`, `read_file`, `write_file`, `upload_file`, `screenshot`, `desktop_input`, `environment_logs`, `environment_diagnostics` |
| Reusable images | `list_images`, `import_image`, `build_image`, `snapshot` |
| Persistent storage | `list_volumes`, `create_volume` |
| Operation results | `list_jobs`, `get_job` |

Each tool advertises its arguments and description through `tools/list`. Read operations are marked read-only; deletion, script execution, file replacement and desktop interaction carry destructive-operation hints for clients that show approvals.

Successful data results are returned as `structuredContent`, with text content for clients that do not use structured results. Lists are wrapped in an `items` object. `screenshot` returns a JPEG image content block. Tool failures return `isError=true` and an explanation; invalid tool names or missing/unknown top-level arguments return a protocol error.

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

Output goes to the job log. A nonzero exit or timeout fails the job and records the exit status. The default time limit is 300 seconds; the allowed range is 1–3500. The script may contain at most 1,000,000 bytes. Scripts are sent over stdin, so large scripts do not rely on Docker command-line argument limits.

Keep a graphical application's launch command from waiting on the application forever. Launch it in the background with redirected input/output if you want to interact through VNC after the script returns. Use `gpu-run` for applications that need hardware OpenGL.

## Read and write files

`read_file` takes a canonical absolute path and reads as the desktop user. Its default encoding is `utf8`; use `base64` for binary data. Each call reads up to 512 KiB, with a default of 64 KiB. Results include `bytes`, `size`, `next_offset` and `eof`; use `offset` and `limit` to page through larger files. Offsets count bytes. A UTF-8 slice that cuts a character or contains binary bytes needs a different boundary or base64 encoding.

`write_file` takes `path` and `data`, with the same encoding choices. It creates missing parent directories and saves the complete file atomically. Existing files require `replace=true`. Writes are limited to 512 KiB and use mode 0600. There is no root file-write option; destination permissions still apply.

Both tools reject path traversal and symlinks. Reads accept regular files only, so devices, sockets and FIFOs cannot turn a bounded read into an endless stream.

`upload_file` is the smaller-file counterpart of the browser upload feature. It accepts standard base64 data and a destination within Downloads or a writable mounted volume. Its MCP limit is 512 KiB; use the streaming HTTP upload endpoint for larger files. See [files in the HTTP API](API.md#files).

## Use the mouse and keyboard

`desktop_input` sends actions through RFB, the VNC protocol, to the same desktop used by the browser. It opens a shared connection, so another viewer can stay connected. Coordinates are pixels in the desktop screenshot, not CSS pixels in a browser window.

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

Supported actions are `move`, `click`, `drag`, `scroll`, `key_press`, `type_text` and `wait`. Clicks support left/middle/right buttons and up to three clicks. Drags have start/end coordinates and a duration of 100–2000 ms. Scroll accepts up/down/left/right and 1–50 steps.

Key chords press the supplied keys together, then release them in reverse order. Special names include Control, Alt, Shift, Super, Enter, Tab, Escape, Backspace, Delete, Insert, Home, End, PageUp, PageDown, arrows and F1–F12. Text supports Unicode; a newline presses Enter and a tab presses Tab.

Requests allow 1–100 actions, up to 10,000 typed characters and at most 10 seconds of combined waits and gesture delays. A single wait is limited to 1000 ms. The whole action batch is validated against the connected desktop's dimensions before input is sent.

The bundled TigerVNC server acknowledges input with RFB Fence. This confirms that the server processed the events; an application may still need time to update. Inspect the next screenshot before making a decision. If a connection fails after input starts, check the screen before repeating an action that could submit, purchase or delete something. A custom desktop VNC server must support RFB 3.8, shared connections and Fence.

## Keep agent work recoverable

MCP uses the same ownership checks, environment locks, jobs and snapshot behavior as the dashboard. An operation on another desktop can proceed independently. A script running in one environment holds that environment's operation lock until it finishes.

Read/write and execution tools operate inside managed desktop containers. They do not expose host shell execution or give desktop containers the host Docker socket. Treat files, logs and screen contents as data from the environment, especially when the desktop is browsing external sites.

Snapshots save filesystem changes; they do not save volumes or running application state. Use a named volume for project files that must survive environment deletion, and save an image before deleting a desktop whose installation you want to reuse.
