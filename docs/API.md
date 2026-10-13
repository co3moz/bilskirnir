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
| GET | `/api/environments/{id}/screenshot` | JPEG image bytes. |
| POST | `/api/environments/{id}/input` | Mouse/keyboard actions through VNC. |
| POST | `/api/environments/{id}/files/read` | Read a bounded file slice as text or base64. |
| POST | `/api/environments/{id}/files/write` | Write text or base64 file data atomically. |
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

The default user is UID/GID 1001:1001; root execution must be explicit. Bash runs with fail-on-error. The response is a job, with output and exit failures in its log. Timeouts range from 1–3500 seconds, defaulting to 300. The working directory is `/home/desktop`; the environment's network and resource limits apply. See [command execution](MCP.md#run-commands-and-install-software) for more detail.

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

For larger uploads, POST the raw file body with `Content-Type: application/octet-stream` to `/api/environments/{id}/files`. Supply `destination`, `name`, `size` and optional `replace` as query parameters:

```bash
curl --cookie bilskirnir.cookies \
  --header 'X-Wsld-Request: 1' \
  --header 'Content-Type: application/octet-stream' \
  --data-binary @notes.txt \
  'http://localhost:8080/api/environments/ENVIRONMENT_ID/files?destination=/home/desktop/Downloads&name=notes.txt&size=FILE_SIZE_IN_BYTES'
```

Replace `FILE_SIZE_IN_BYTES` with the exact size. URL-encode filenames and destination paths when necessary. Streaming uploads are limited by `UPLOAD_MAX_MB` and restricted to Downloads or a writable mounted volume. They return the saved path and byte count.

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

The response includes `applied_actions`, `width` and `height`. Coordinates use the remote desktop's pixels. Other actions are:

```json
[
  {"type":"move","x":100,"y":100},
  {"type":"drag","from_x":100,"from_y":100,"to_x":300,"to_y":200,"button":"left","duration_ms":300},
  {"type":"scroll","x":300,"y":200,"direction":"down","steps":3},
  {"type":"wait","milliseconds":500}
]
```

Put the actions you need in your request's `actions` array. Batches allow 1–100 actions and at most 10 seconds of combined delays. Input travels through a shared RFB connection with Fence acknowledgement; it does not disconnect the browser viewer. See the [MCP input guide](MCP.md#use-the-mouse-and-keyboard) for key names and limits.

## Limits and results

JSON request bodies are limited to 1,100,000 bytes. Binary uploads have their own configured limit. Logs retain the last 256 KiB. Screenshot requests share two processing slots, uploads allow two simultaneous transfers and desktop input allows four; operations on one environment still use its resource lock.

File and input operations return their result directly. Creation, lifecycle changes, scripts and image preparation return jobs. A VNC acknowledgement confirms that input was processed, not that an application has finished responding; inspect a screenshot or application result before the next step.
