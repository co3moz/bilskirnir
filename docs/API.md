# HTTP API

[Back to the README](../README.md) · [MCP guide](MCP.md)

The dashboard and MCP server share Bilskirnir's backend operations. You can also call the HTTP API directly from a script or another application to manage desktops and build your own interface.

All paths below are relative to your installation, for example `http://localhost:8080`. Request bodies and results are JSON unless noted otherwise. Errors return an HTTP error status and an `error` message.

## Sign in

The HTTP API uses the same administrator session as the dashboard. Sign in and keep the returned cookie:

```bash
curl --cookie-jar bilskirnir.cookies \
  --header 'Content-Type: application/json' \
  --header 'X-Wsld-Request: 1' \
  --data '{"username":"admin","password":"YOUR_PASSWORD"}' \
  http://localhost:8080/api/login
```

Send that cookie with subsequent requests. POST and DELETE requests also need `X-Wsld-Request: 1`. If you send an Origin header, it must match the request's host and port. These request checks still apply when `AUTH_MODE=none`; only sign-in and the cookie become unnecessary.

The `wsld_session` cookie and `X-Wsld-Request` header retain their names for compatibility. The MCP bearer token authenticates `/mcp`, not these API routes. Sessions last 12 hours; `/api/logout` revokes the session.

## Endpoints

| Method | Path | Result or purpose |
| --- | --- | --- |
| GET | `/api/session` | Sign-in mode and current authentication state; available before login. |
| POST | `/api/login` | Sign in with `username` and `password`. |
| POST | `/api/logout` | Sign out. |
| GET | `/api/settings` | Defaults and supported features, without credentials. |
| GET | `/api/system` | Docker connection and host summary. |
| GET | `/api/profiles` | Desktop profile catalog and image availability. |
| POST | `/api/profiles/{profile_id}/build` | Build a supplied profile; returns a job. |
| GET | `/api/environments` | Managed environments, state and health. |
| POST | `/api/environments` | Create an environment; returns a job. |
| GET | `/api/environments/{id}` | One environment's configuration, state and health. |
| POST | `/api/environments/{id}/action` | Start, stop or restart; returns a job. |
| POST | `/api/environments/{id}/retry` | Retry partial setup; returns a job. |
| POST | `/api/environments/{id}/internet` | Set internet egress; returns a job. |
| DELETE | `/api/environments/{id}` | Delete the container and networks; returns a job. |
| POST | `/api/environments/{id}/exec` | Run a script; returns a job. |
| POST | `/api/environments/{id}/snapshot` | Save filesystem changes as an image; returns a job. |
| GET | `/api/environments/{id}/logs` | Bounded container log output. |
| GET | `/api/environments/{id}/diagnostics` | Desktop and graphics diagnostics. |
| GET | `/api/environments/{id}/screenshot` | JPEG at desktop resolution; accepts `max_width`, `region` and `window_id`. |
| POST | `/api/environments/{id}/input` | Mouse/keyboard actions through VNC. |
| POST | `/api/environments/{id}/act-and-observe` | Input actions followed by a screenshot, as JSON. |
| GET | `/api/environments/{id}/applications` | Launched applications and their state. |
| POST | `/api/environments/{id}/applications` | Launch an application in the background. |
| GET | `/api/environments/{id}/applications/{application_id}` | One application's state, pid and exit status. |
| GET | `/api/environments/{id}/applications/{application_id}/logs` | Tail of the application's log. |
| POST | `/api/environments/{id}/applications/{application_id}/stop` | Stop the application. |
| GET | `/api/environments/{id}/applications/{application_id}/diagnostics` | Process tree, resources and latest log lines. |
| GET | `/api/environments/{id}/applications/{application_id}/metrics` | CPU and memory sampled over `duration_ms`. |
| POST | `/api/environments/{id}/wait` | Wait for a window, application state, log text or a stable screen. |
| POST | `/api/environments/{id}/reset` | Recreate the container from its source image; returns a job. |
| GET | `/api/environments/{id}/windows` | Visible X11 windows and the active window. |
| POST | `/api/environments/{id}/windows/{window_id}/focus` | Raise and focus a window. |
| POST | `/api/environments/{id}/windows/{window_id}/geometry` | Move or resize a window. |
| POST | `/api/environments/{id}/files/read` | Read a bounded file slice as text or base64. |
| POST | `/api/environments/{id}/files/write` | Write text or base64 file data atomically. |
| POST | `/api/environments/{id}/files/list` | List a directory without following symlinks. |
| POST | `/api/environments/{id}/files/move` | Rename or move within the same volume. |
| POST | `/api/environments/{id}/files/delete` | Delete a file, symlink or directory. |
| POST | `/api/environments/{id}/files` | Stream a binary upload. |
| GET | `/api/images` | Saved images, snapshots, ancestry and availability. |
| POST | `/api/images` | Import a desktop-compatible local image using `name` and `reference`. |
| POST | `/api/images/build` | Build an image using `name`, `base_image`, `script` and optional `internet`; returns a job. |
| GET | `/api/volumes` | Docker named volumes. |
| POST | `/api/volumes` | Create a named volume using `name`. |
| GET | `/api/jobs` | Latest 100 job summaries. |
| GET | `/api/jobs/{job_id}` | Job status, log and result. |
| GET | `/healthz` | Basic manager health; does not require sign-in. |

The VNC and media WebSocket routes are `/api/environments/{id}/vnc` and `/api/environments/{id}/audio/{direction}`, where direction is `output` or `input`. They use the browser session and require a matching Origin. VNC uses the binary subprotocol. Most automation clients can use screenshots and `/input` instead of implementing a VNC viewer.

## Create a desktop and follow the job

```bash
curl --cookie bilskirnir.cookies \
  --header 'Content-Type: application/json' \
  --header 'X-Wsld-Request: 1' \
  --data '{"name":"Agent desktop","image":"ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04","gpu":true,"internet":true}' \
  http://localhost:8080/api/environments
```

The response contains `job_id`. Poll its detail endpoint:

```bash
curl --cookie bilskirnir.cookies http://localhost:8080/api/jobs/JOB_ID
```

A job is finished when its status is `completed`, `failed` or `interrupted`. A successful creation puts `environment_id` in `result`. Read the log for a failed job; after interruption, inspect the resources before retrying.

Creation accepts `name`, `image` and these optional fields:

| Field | Default | Meaning |
| --- | --- | --- |
| `resolution` | `1920x1080` | Desktop size, from 320x240 to 7680x4320. |
| `gpu` | `false` | Request GPU access supported by the installation. |
| `internet` | `true` | Attach the Docker egress network. |
| `audio`, `microphone` | `false` | Enable supported browser media; microphone also needs audio. |
| `sudo` | `true` | Allow the desktop user to use sudo. |
| `cpus`, `memory_mb` | No limit | Positive CPU count and at least 256 MiB of memory. |
| `shm_mb` | `512` | Shared memory size, at least 64 MiB. |
| `mounts` | `[]` | Objects with `volume`, absolute `target` and optional `readonly`. |

Start/stop/restart uses `{"action":"start"}`, internet uses `{"enabled":false}` and snapshot uses `{"name":"Editor installed","description":"Optional note"}`. Actions on an environment are serialized. Deleting it preserves named volumes and saved images, but removes unsaved changes in its container layer.

## Run a script

POST to `/api/environments/{id}/exec`:

```json
{
  "script": "apt-get update\napt-get install -y git",
  "root": true,
  "timeout_seconds": 600
}
```

The default user is UID/GID 1001:1001; root execution must be explicit. Bash runs with fail-on-error and with standard input closed, so a command that reads input receives end-of-file instead of consuming the rest of the script. The response is a job, with output and exit failures in its log. Timeouts range from 1–3500 seconds, defaulting to 300. The working directory is `/home/desktop`; the environment's network and resource limits apply. The script holds the environment's operation lock until it finishes, so start graphical programs with the [applications API](#applications-and-windows) instead of a script. See [command execution](MCP.md#run-commands-and-install-software) for more detail.

## Files

File reads and writes use the desktop user's permissions and require a running environment. They reject symlinks and noncanonical paths. Reads accept regular files only.

Read request:

```json
{"path":"/home/desktop/project/config.json","encoding":"utf8","offset":0,"limit":65536}
```

The response includes `data`, `encoding`, `bytes`, `size`, `offset`, `next_offset` and `eof`. `limit` is 1–524288 bytes. Use `encoding:"base64"` for binary files or slices that split UTF-8 characters.

Write request:

```json
{"path":"/home/desktop/project/config.json","data":"{\"enabled\":true}\n","encoding":"utf8","replace":false}
```

Writes are atomic, create missing parent directories and use mode 0600. Existing files return 409 unless replacement is explicit. The decoded file limit is 512 KiB. Use `encoding:"base64"` for binary data.

`POST .../files/list` with `{"path":"/home/desktop/project","limit":500}` returns `items` with `name`, `type` (`file`, `directory`, `symlink` or `special`), `size`, `modified_at` and `mode`, plus `total` and `truncated`. `POST .../files/move` with `{"source":...,"destination":...,"replace":false}` renames within the same volume and returns 409 when the destination exists. `POST .../files/delete` with `{"path":...,"recursive":false}` removes a file, a symlink itself or a directory; top-level directories, `/home/desktop` and mounted volume roots are refused. Use the read route with base64 encoding to download a file.

For larger uploads, POST the raw file body with `Content-Type: application/octet-stream` to `/api/environments/{id}/files`. Supply `destination`, `name`, `size` and optional `replace` as query parameters:

```bash
curl --cookie bilskirnir.cookies \
  --header 'X-Wsld-Request: 1' \
  --header 'Content-Type: application/octet-stream' \
  --data-binary @notes.txt \
  'http://localhost:8080/api/environments/ENVIRONMENT_ID/files?destination=/home/desktop/Downloads&name=notes.txt&size=FILE_SIZE_IN_BYTES'
```

Replace `FILE_SIZE_IN_BYTES` with the exact size. URL-encode filenames and destination paths when necessary. Streaming uploads are limited by `UPLOAD_MAX_MB` and restricted to Downloads or a writable mounted volume. They return the saved path and byte count.

## Screenshots

`GET /api/environments/{id}/screenshot` returns a JPEG at the desktop's resolution, so its pixels match `/input` coordinates. `max_width` (16–7680) scales the image down, `region=x,y,width,height` crops in desktop pixels and `window_id` captures one window from the windows route. The response carries `X-Image-Width`, `X-Image-Height` and `X-Captured-At` (Unix milliseconds) headers. The dashboard requests `max_width=640` for previews.

`POST /api/environments/{id}/act-and-observe` takes `{"actions":[...],"settle_ms":500,"capture":{"max_width":960}}`, performs the actions, waits `settle_ms` (0–10000) and returns `{"input":{...},"screenshot":{"width","height","captured_at","bytes","jpeg_base64"}}`.

## Mouse and keyboard

POST to `/api/environments/{id}/input` with an `actions` array:

```json
{
  "actions": [
    {"type":"click","x":400,"y":250,"button":"left","count":1},
    {"type":"key_press","keys":["Control","a"]},
    {"type":"type_text","text":"Hello"},
    {"type":"key_press","keys":["Enter"]}
  ]
}
```

The response includes `applied_actions`, `width`, `height`, the final `pointer` position, `delay_ms` and `released`, which lists keys and buttons the request left held and the manager released. Coordinates use the remote desktop's pixels. Other actions are:

```json
[
  {"type":"move","x":100,"y":100},
  {"type":"drag","from_x":100,"from_y":100,"to_x":300,"to_y":200,"button":"left","duration_ms":300},
  {"type":"scroll","x":300,"y":200,"direction":"down","steps":3},
  {"type":"wait","milliseconds":500},
  {"type":"move_relative","dx":40,"dy":-10,"steps":4,"duration_ms":200},
  {"type":"button_down","x":100,"y":100,"button":"left"},
  {"type":"button_up","button":"left"},
  {"type":"key_down","keys":["w","Shift"]},
  {"type":"key_up","keys":["Shift"]},
  {"type":"hold_keys","keys":["space"],"milliseconds":250},
  {"type":"release_all"}
]
```

Put the actions you need in your request's `actions` array. Batches allow 1–100 actions, single waits or holds of at most 10 seconds and at most 30 seconds of combined delays. Keys and buttons held by `key_down` and `button_down` stay pressed for later actions in the same request and are released when the request ends. `move_relative` is emulated with absolute positions from the pointer location reported by the desktop. Input travels through a shared RFB connection with Fence acknowledgement; it does not disconnect the browser viewer. See the [MCP input guide](MCP.md#use-the-mouse-and-keyboard) for key names and limits.

## Applications and windows

POST `{"command":"glxgears","args":["-info"],"cwd":"/home/desktop","gpu":true,"env":{"MY_SETTING":"1"},"name":"gears"}` to `/api/environments/{id}/applications`. The program runs in the background as the desktop user; `gpu` runs it through `gpu-run` and needs a GPU-enabled environment. The response includes `application_id`, `state`, `pid` and, once the program ends, `exit_code` or the terminating `signal`. States are `starting`, `running`, `exited`, `failed` and `unknown`.

`GET .../applications/{application_id}/logs?tail_bytes=65536` returns the end of the combined stdout/stderr log, which keeps about the latest 1 MiB. `POST .../applications/{application_id}/stop` with an optional `{"grace_seconds":5}` body sends SIGTERM to the process group, then SIGKILL. Supervisor files live under `/tmp` in the desktop and disappear when it restarts, after which the state is `unknown`.

`GET .../applications/{application_id}/diagnostics` returns the application's state, exit status, its session's processes with memory use, open file count, thread count and the latest log lines. `GET .../applications/{application_id}/metrics?duration_ms=1000` samples CPU time and resident memory of those processes over the window (100–10000 ms) and reports container memory from cgroups; frame rate is not available.

`POST .../wait` with `{"window_title":"Game","application_id":"...","application_state":"running","log_contains":"ready","screen_stable_ms":1000,"region":"0,0,800,600","timeout_ms":10000,"interval_ms":500}` polls until every supplied condition holds and returns `satisfied`, `elapsed_ms` and the last observation; after `timeout_ms` (100–60000) it returns `satisfied:false` rather than an error. `application_state` defaults to `exited`, which also matches `failed`.

`POST .../reset` recreates the container from the environment's saved source image ID and returns a job. Unsaved changes in the container layer are discarded; named volumes, networks and the environment ID are kept, and the data inside volumes is not touched. The reset is refused when the source image is missing.

`GET .../windows` lists visible windows with `window_id`, `title`, `pid`, `x`, `y`, `width`, `height` and `active`. `POST .../windows/{window_id}/focus` activates a window, and `POST .../windows/{window_id}/geometry` with `x`,`y` and/or `width`,`height` moves or resizes it and returns the resulting geometry. Window IDs are hexadecimal X11 IDs such as `0x3a00007`. The window routes, window screenshots and relative pointer movement need `xdotool` and `wmctrl` in the desktop image.

## Limits and results

JSON request bodies are limited to 1,100,000 bytes. Binary uploads have their own configured limit. Logs retain the last 256 KiB; application logs keep about 1 MiB. Screenshot requests share two processing slots, uploads allow two simultaneous transfers and desktop input allows four; operations on one environment still use its resource lock, and a launched application does not hold it while it runs.

MCP JSON responses have an explicit content length and are limited to 8 MiB. This includes screenshots returned as base64 image content. A [stdio bridge](MCP.md#connect-through-stdio-on-windows) is bundled for clients that cannot use the HTTP connection directly.

File and input operations return their result directly. Creation, lifecycle changes, scripts and image preparation return jobs. A VNC acknowledgement confirms that input was processed, not that an application has finished responding; inspect a screenshot or application result before the next step.
