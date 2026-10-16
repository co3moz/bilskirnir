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
| GET | `/api/profiles` | Desktop profile catalog and whether each image is downloaded. |
| POST | `/api/profiles/{profile_id}/pull` | Download a supplied profile's published image, or a newer one; returns a job. |
| POST | `/api/profiles/{profile_id}/build` | Build a supplied profile; returns a job. |
| GET | `/api/environments` | Managed environments, state and health. |
| POST | `/api/environments` | Create an environment, with values for its image's [template](#image-templates) in `parameters`; returns a job. |
| GET | `/api/environments/{id}` | One environment's configuration, state and health. |
| POST | `/api/environments/{id}/action` | Start, stop or restart; returns a job. |
| POST | `/api/environments/{id}/retry` | Retry partial setup or a template setup that did not finish; returns a job. |
| POST | `/api/environments/{id}/internet` | Set internet egress; returns a job. |
| POST | `/api/environments/{id}/settings` | Change settings after creation; `dry_run` reports what restarts the desktop. Returns a job. |
| DELETE | `/api/environments/{id}` | Delete the container and networks; returns a job. |
| POST | `/api/environments/{id}/exec` | Run a script; returns a job. |
| POST | `/api/environments/{id}/snapshot` | Save filesystem changes as an image; returns a job. |
| GET | `/api/environments/{id}/logs` | Bounded container log output. |
| GET | `/api/environments/{id}/diagnostics` | Desktop and graphics diagnostics. |
| GET | `/api/environments/{id}/screenshot` | JPEG at desktop resolution; accepts `max_width`, `region` and `window_id`. |
| POST | `/api/environments/{id}/input` | Mouse/keyboard actions through VNC. |
| POST | `/api/environments/{id}/act-and-observe` | Input actions followed by a screenshot, as JSON. |
| POST | `/api/environments/{id}/screenshots` | Save a screenshot inside the desktop and return its `download_url`. |
| GET | `/api/environments/{id}/applications` | Launched applications and their state. |
| POST | `/api/environments/{id}/applications` | Launch an application in the background. |
| GET | `/api/environments/{id}/applications/{application_id}` | One application's state, pid and exit status. |
| GET | `/api/environments/{id}/applications/{application_id}/logs` | Tail of the application's log. |
| POST | `/api/environments/{id}/applications/{application_id}/stop` | Stop the application. |
| GET | `/api/environments/{id}/applications/{application_id}/diagnostics` | Process tree, resources and latest log lines. |
| GET | `/api/environments/{id}/applications/{application_id}/metrics` | CPU and memory sampled over `duration_ms`. |
| POST | `/api/environments/{id}/wait` | Wait for a window, application state, log text or a stable screen. |
| POST | `/api/environments/{id}/reset` | Recreate the container from its source image; returns a job. |
| GET | `/api/environments/{id}/recordings` | Recordings and their state. |
| POST | `/api/environments/{id}/recordings` | Start recording desktop frames. |
| POST | `/api/environments/{id}/recordings/{recording_id}/stop` | Stop a recording. |
| POST | `/api/environments/{id}/artifacts` | Package recordings and application logs as a tar.gz. |
| POST | `/api/environments/{id}/audio-capture` | Record desktop or microphone audio as WAV. |
| POST | `/api/environments/{id}/audio-inject` | Play WAV data into the microphone or speakers. |
| GET | `/api/environments/{id}/terminals` | Terminal sessions and their state. |
| POST | `/api/environments/{id}/terminals` | Open an interactive shell. |
| POST | `/api/environments/{id}/terminals/{terminal_id}/input` | Type into a terminal. |
| GET | `/api/environments/{id}/terminals/{terminal_id}/output` | Read terminal output from an offset. |
| POST | `/api/environments/{id}/terminals/{terminal_id}/close` | Hang up a terminal. |
| GET | `/api/environments/{id}/ui` | AT-SPI accessibility tree of applications. |
| POST | `/api/environments/{id}/gamepad` | Feed the virtual controller. |
| GET | `/api/environments/{id}/windows` | Visible X11 windows and the active window. |
| POST | `/api/environments/{id}/windows/{window_id}/focus` | Raise and focus a window. |
| POST | `/api/environments/{id}/windows/{window_id}/geometry` | Move or resize a window. |
| POST | `/api/environments/{id}/files/read` | Read a bounded file slice as text or base64. |
| POST | `/api/environments/{id}/files/write` | Write text or base64 file data atomically. |
| POST | `/api/environments/{id}/files/list` | List a directory without following symlinks. |
| POST | `/api/environments/{id}/files/move` | Rename or move within the same volume. |
| POST | `/api/environments/{id}/files/delete` | Delete a file, symlink or directory. |
| GET | `/api/environments/{id}/files/download` | Stream a file with `path`; limited by `UPLOAD_MAX_MB`. |
| POST | `/api/environments/{id}/files` | Stream a binary upload. |
| GET | `/api/images` | Saved images, snapshots, ancestry and availability. A script build carries `build` with its `base_image` and `internet`, and a template its parameters without the script. |
| POST | `/api/images` | Import a desktop-compatible local image using `name` and `reference`. |
| POST | `/api/images/build` | Build an image using `name`, `base_image`, `script` and optional `internet` and `template`; returns a job. |
| GET | `/api/images/{id}/config` | The `name`, `base_image`, `internet` and `script` a script build ran, with the image's `template` if it has one, a body `/api/images/build` accepts as it is. `id` is the record's `id`, not its Docker `image_id`. `404` for snapshots, imports and builds from earlier releases. |
| GET, PUT, DELETE | `/api/images/{id}/template` | Read, set or remove an image's [template](#image-templates). `id` is the record's `id`. |
| GET | `/api/images/{id}/archive` | Download a saved image as a gzip-compressed `docker save` archive; the transfer is recorded as a job. |
| POST | `/api/images/import` | Stream a `.tar.gz` or `.tar` image archive with `size` and optional `name` and `description`; returns a job. |
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

Creating a desktop or building an image from a supplied profile that is not downloaded yet pulls it first, in the same job. While the download runs, the job carries `progress`: `current` and `total` compressed bytes, where `total` is `null` until the size of every remaining layer is known, and `layers_done` of `layers`. Layers already on the host count as done and are not part of the byte total. The field is removed when the download ends. A download that fails is tried up to three times in the same job, unless Docker refused it, such as for an unknown image.

Creation accepts `name`, `image` and these optional fields:

| Field | Default | Meaning |
| --- | --- | --- |
| `resolution` | `1920x1080` | Desktop size, from 320x240 to 7680x4320. |
| `gpu` | `false` | Request GPU access supported by the installation. |
| `internet` | `true` | Attach the Docker egress network. |
| `audio`, `microphone` | `false` | Enable supported browser media; microphone also needs audio. |
| `sudo` | `true` | Allow the desktop user to use sudo. |
| `gamepad` | `false` | Attach `/dev/uinput` so the gamepad route can create a virtual controller. |
| `cpus`, `memory_mb` | No limit | Positive CPU count and at least 256 MiB of memory. |
| `shm_mb` | `512` | Shared memory size, at least 64 MiB. |
| `mounts` | `[]` | Objects with `volume`, absolute `target` and optional `readonly`. |
| `parameters` | `{}` | Values for the [template](#image-templates) parameters of the image, by name. |

## Change settings after creation

`POST .../settings` takes the creation fields to change, except `image`; fields that are left out keep their value. `name`, `internet` and a new or changed `cpus` or `memory_mb` apply to the running container. `resolution`, `gpu`, `audio`, `microphone`, `sudo`, `gamepad`, `shm_mb`, `mounts` and removing a limit (`null`) cannot be changed on an existing Docker container: the job stops the desktop, commits its container layer to an image tagged `bilskirnir/environment:JOB_ID`, creates a new container from that image with the new settings and starts it again if it was running. Running applications close; files in the container layer are kept, and named volumes are not touched. If the new container cannot start, the previous one is restored. A reset still recreates the desktop from its original source image, and deleting the environment removes the committed image unless a snapshot builds on it.

With `"dry_run": true` the request is validated and answered at once with `live` and `restart`, the changed fields grouped by how they apply, without a job:

```bash
curl --cookie bilskirnir.cookies \n  --header 'Content-Type: application/json' \n  --header 'X-Wsld-Request: 1' \n  --data '{"audio":true,"dry_run":true}' \n  http://localhost:8080/api/environments/ENVIRONMENT_ID/settings
```

A feature the installation has switched off cannot be turned on. The job's result repeats the `live` and `restart` lists.

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

## Image templates

A saved image can carry a template: parameters that a new environment from it asks for, and a setup script that receives them. `PUT /api/images/{id}/template` sets or replaces it, `GET` returns it with its script and `DELETE` removes it; `id` is the record's `id`. Image lists and job results show the template without its script.

```json
{
  "parameters": [
    {"name": "UNITY_EMAIL", "label": "Unity email", "required": true},
    {"name": "UNITY_PASSWORD", "label": "Unity password", "required": true, "secret": true},
    {"name": "UNITY_EDITOR", "label": "Editor", "type": "choice", "allow_custom": true,
     "options": [{"value": "6000.0.23f1", "label": "Unity 6"}, {"value": "2022.3.50f1", "label": "2022 LTS"}],
     "default": "6000.0.23f1"},
    {"name": "BUILD_JOBS", "label": "Build jobs", "type": "number", "display": "slider",
     "min": 1, "max": 16, "default": "4"},
    {"name": "PLATFORMS", "label": "Platforms", "type": "choices",
     "options": [{"value": "linux"}, {"value": "windows"}, {"value": "webgl"}], "default": "linux"}
  ],
  "script": "set -eu\nwhile read -r platform; do echo \"$UNITY_EDITOR for $platform\"; done <<< \"$PLATFORMS\"\n",
  "root": false,
  "timeout_seconds": 600
}
```

| Field | Default | Meaning |
| --- | --- | --- |
| `parameters` | `[]` | Up to 32 objects with `name` and optional `label`, `type`, `display`, `required`, `default` and the fields of their type, below. |
| `script` | Required | Bash script of at most 1,000,000 bytes, run with fail-on-error and standard input closed. |
| `root` | `false` | Run as root instead of the desktop user, UID/GID 1001:1001. |
| `timeout_seconds` | `600` | From 1 to 3500 seconds. |

A parameter's `name` is the environment variable that carries its value to the script: at most 64 capital letters, digits and underscores, starting with a letter. Names that the shell or the desktop session relies on, such as `PATH`, `HOME`, `DISPLAY` or names starting with `LD_`, `XDG_` or `DESKTOP_`, are refused. A `label` is what the dashboard asks for, the name when empty. Values are text of at most 16 KiB. A secret parameter cannot have a default, because image lists return the template.

Every value reaches the script as text, so a parameter's `type` decides what text it accepts and how the dashboard asks for it. `default` is written the same way the script receives it.

| `type` | `display` | The script receives | Fields of the type |
| --- | --- | --- | --- |
| `text` (default) | `input` (default), `multiline` | The text; one line unless `multiline` | `secret` |
| `number` | `input` (default), `slider` | A plain decimal such as `42` or `-1.5`, without exponents | `min`, `max`, `step`; a slider needs `min` and `max`, and steps count from `min` or from 0 |
| `boolean` | `toggle` (default), `checkbox` | `true` or `false`; `false` when left out | |
| `choice` | `select` (default), `radio` | One option's `value` | `options`, `allow_custom` to also accept other text |
| `choices` | `checkboxes` | The chosen values, one per line, each at most once | `options` |

`options` holds 1 to 100 objects with a `value` of one line and an optional `label` the dashboard shows instead. A field of another type, a display the type does not have or a default the parameter would refuse is an error. `required` means a value must be given; for a boolean, that it must be `true`, as for an agreement. Read several choices in bash with `while read -r item; do ...; done <<< "$PLATFORMS"`.

`POST /api/environments` takes the values in `parameters`, such as `{"UNITY_EMAIL":"dev@example.com","UNITY_PASSWORD":"..."}`. A value that is left out uses the default. Every parameter reaches the script, an optional one without a value as an empty string, so `set -u` does not trip on it. An unknown name or a missing required value fails the request. The creation job waits until the desktop reports ready, for at most five minutes, then runs the script in the desktop session's environment (`HOME`, `DISPLAY`, `XDG_RUNTIME_DIR`) and completes only when the script succeeds.

The environment pins the template and the values, so editing the image later does not change what it runs. Its `setup` field reports `status` (`pending`, `running`, `completed` or `failed`), the `image` record the template came from, the values of ordinary `parameters` and the names of the `secrets`. Secret values are never returned, and the setup job's log shows `***` in their place. The manager keeps them in its database until the environment is deleted, so that the setup can run again: after a reset, and when retry recreates a missing container from the source image. A settings change keeps the container's files and does not run it. A script that fails leaves the desktop running with `last_error` set; retry runs it again.

A snapshot keeps the template its environment was created with, so a desktop created from the snapshot asks for the values again. Write the script so it also works on a desktop where it already ran. The build configuration and an image archive carry the template too.

The job stops reading the script's output a second after the script exits. A program the script leaves running fails if it writes to that output later, so start long-running programs with the [applications API](#applications-and-windows), or redirect their output.

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

`POST .../files/list` with `{"path":"/home/desktop/project","limit":500}` returns `items` with `name`, `type` (`file`, `directory`, `symlink` or `special`), `size`, `modified_at` and `mode`, plus `total` and `truncated`. `POST .../files/move` with `{"source":...,"destination":...,"replace":false}` renames within the same volume and returns 409 when the destination exists. `POST .../files/delete` with `{"path":...,"recursive":false}` removes a file, a symlink itself or a directory; top-level directories, `/home/desktop` and mounted volume roots are refused. `GET .../files/download?path=...` streams a regular file with `Content-Length`, a guessed `Content-Type` and an attachment `Content-Disposition`; it refuses symlinks and files larger than `UPLOAD_MAX_MB`. Responses that describe a saved file, such as `download_file`, `export_artifacts` and saved screenshots, include this route as a root-relative `download_url` for the dashboard session.

For larger uploads, POST the raw file body with `Content-Type: application/octet-stream` to `/api/environments/{id}/files`. Supply `destination`, `name`, `size` and optional `replace` as query parameters:

```bash
curl --cookie bilskirnir.cookies \
  --header 'X-Wsld-Request: 1' \
  --header 'Content-Type: application/octet-stream' \
  --data-binary @notes.txt \
  'http://localhost:8080/api/environments/ENVIRONMENT_ID/files?destination=/home/desktop/Downloads&name=notes.txt&size=FILE_SIZE_IN_BYTES'
```

Replace `FILE_SIZE_IN_BYTES` with the exact size. URL-encode filenames and destination paths when necessary. Streaming uploads are limited by `UPLOAD_MAX_MB` and restricted to Downloads or a writable mounted volume. They return the saved path and byte count.

## Image archives

`GET /api/images/{id}/archive` streams a saved image, where `id` is the record's `id` from `GET /api/images`. The download is a gzip-compressed `docker save` archive of the image ID, so it names no image. Its first entry, `bilskirnir.json`, holds the record's name, description, kind and desktop profile, and a script build's configuration, so `/api/images/{id}/config` works for the imported record too. Docker's own `docker load` reads the archive too and reports the loaded image ID.

```bash
curl --cookie bilskirnir.cookies --output unity.tar.gz \
  http://localhost:8080/api/images/IMAGE_RECORD_ID/archive
```

An archive holds every layer, the base desktop included, and its size is not known in advance. Docker prepares the whole image before its first layer arrives. The transfer is an `image-export` job, named in the `X-Job-Id` response header, whose `progress` counts the bytes sent. If Docker fails partway, the download ends with an error instead of a short file.

`POST /api/images/import` takes the archive as an `application/octet-stream` body: an exported `.tar.gz` or a plain `docker save` `.tar`. Supply the exact `size` and optionally `name` and `description` as query parameters; they default to the values stored in the archive. `--upload-file` streams the file instead of reading it into memory:

```bash
curl --cookie bilskirnir.cookies \
  --header 'X-Wsld-Request: 1' \
  --header 'Content-Type: application/octet-stream' \
  --request POST --upload-file unity.tar.gz \
  'http://localhost:8080/api/images/import?size=FILE_SIZE_IN_BYTES&name=Unity%20workspace'
```

The archive streams into `docker load` as it arrives. On the way, the manager removes every tag and image name the archive carries, so an import cannot replace a profile image or any other existing tag. Docker can register the image only after the archive has passed these checks:

- It holds exactly one image, with its `manifest.json` and configuration.
- The image has the `io.wsl-desktop.desktop=true` label.
- It is at most `IMAGE_ARCHIVE_MAX_MB` after decompression.

The response arrives once the whole archive has passed: `{"job_id":...}` while Docker finishes loading in that `image-import` job, or status 400 with the reason it was refused. A `size` above the limit is refused at once with 413, and a third transfer while two archives are moving with 429. The finished job's result is the new image record, of kind `archive`, with the reference `bilskirnir/imported:<id>`.

## Screenshots

`GET /api/environments/{id}/screenshot` returns a JPEG at the desktop's resolution, so its pixels match `/input` coordinates. `max_width` (16–7680) scales the image down, `region=x,y,width,height` crops in desktop pixels and `window_id` captures one window from the windows route. The response carries `X-Image-Width`, `X-Image-Height` and `X-Captured-At` (Unix milliseconds) headers. The dashboard requests `max_width=640` for previews.

`POST /api/environments/{id}/screenshots` with `{"path":"/home/desktop/Downloads/clock.jpg","replace":false,"capture":{"max_width":960}}` captures with the same options and writes the JPEG inside the desktop as the desktop user, under the upload destination rules. It returns `path`, `bytes`, `width`, `height`, `captured_at` and `download_url`; an existing file returns 409 unless `replace` is set.

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

`POST .../recordings` with `{"fps":2,"max_seconds":60,"max_width":960,"region":"0,0,1280,720","name":"level 1"}` starts capturing JPEG frames into `/tmp/bilskirnir-recordings/{recording_id}` inside the desktop and returns `recording_id`. Recording stops at `max_seconds`, after 3000 frames or 256 MiB, or on `POST .../recordings/{recording_id}/stop`, which returns `frames`, `bytes`, `started_at`, `ended_at` and `reason`. Frames are named `frame-00000-<unix ms>.jpg` and can be read with the file routes; stopping also writes `recording.avi`, an MJPEG video playable with VLC, mpv or ffmpeg, whose details are returned under `video`. `POST .../artifacts` with `{"destination":"/home/desktop/Downloads","name":"run.tar.gz","recording_ids":[...],"application_ids":[...],"replace":false}` writes an archive containing `recordings/<id>/` frames, `applications/<id>/` logs and a `manifest.json`; the destination follows the upload rules and existing archives return 409 unless `replace` is set. Recording files live in tmpfs and disappear when the desktop restarts.

## Audio, terminals, accessibility and gamepad

`POST .../audio-capture` with `{"seconds":3,"source":"desktop"}` records what the desktop plays (or, with `microphone`, what reaches its microphone) as 16-bit stereo 48 kHz WAV and returns `wav_base64`, `peak`, `rms` and `silent`. `POST .../audio-inject` with `{"data_base64":"...","target":"microphone"}` plays 16-bit PCM WAV data into the desktop's microphone (needs the microphone option) or, with `speakers`, into its audio output. Both need audio enabled for the environment.

`POST .../terminals` with an optional `{"cwd":"/home/desktop","root":false}` opens bash on a pseudo-terminal and returns `terminal_id`. `POST .../terminals/{terminal_id}/input` with `{"text":"ls","press_enter":true}` types into it; `GET .../terminals/{terminal_id}/output?offset=0&limit=65536&strip_ansi=true` returns `output`, `next_offset`, `more` and the shell state, keeping about the latest 1 MiB. `POST .../terminals/{terminal_id}/close` hangs up the shell. Root shells are an explicit administrator choice.

`GET .../ui?pid=1234&name=&max_depth=6&max_nodes=500` returns the AT-SPI accessibility tree of applications that expose one (GTK, Electron, Qt with `QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1`): roles, names, states, screen extents, text and actions. Games usually expose nothing.

`POST .../gamepad` with `{"actions":[{"type":"button","button":"a"},{"type":"axis","axis":"left_x","value":0.5},{"type":"wait","milliseconds":100},{"type":"reset"}]}` drives a virtual Xbox-style controller in an environment created with `gamepad:true`. The device is created on first use as `/dev/input/eventN` inside the desktop and keeps button and axis state between requests until `reset` or a desktop restart. Buttons: a, b, x, y, lb, rb, back, start, guide, ls, rs. Axes: left_x, left_y, right_x, right_y (-1 to 1), left_trigger, right_trigger (0 to 1), dpad_x, dpad_y (-1, 0, 1). SDL applications that use udev can set `SDL_JOYSTICK_DISABLE_UDEV=1` to scan `/dev/input` directly.

`GET .../windows` lists visible windows with `window_id`, `title`, `pid`, `x`, `y`, `width`, `height` and `active`. `POST .../windows/{window_id}/focus` activates a window, and `POST .../windows/{window_id}/geometry` with `x`,`y` and/or `width`,`height` moves or resizes it and returns the resulting geometry. Window IDs are hexadecimal X11 IDs such as `0x3a00007`. The window routes, window screenshots and relative pointer movement need `xdotool` and `wmctrl` in the desktop image.

## Limits and results

JSON request bodies are limited to 1,100,000 bytes. Binary uploads have their own configured limit. Logs retain the last 256 KiB; application logs keep about 1 MiB. Screenshot requests share two processing slots, uploads allow two simultaneous transfers, image archive exports and imports two more, and desktop input allows four; operations on one environment still use its resource lock, and a launched application does not hold it while it runs.

MCP JSON responses are limited to 8 MiB and, like every `/mcp` response, use chunked transfer encoding, which survives the [HTTP scanning of some Windows security products](MCP.md#broken-responses-from-localhost-on-windows). This includes screenshots returned as base64 image content. A [stdio bridge](MCP.md#connect-through-stdio-on-windows) is bundled for clients that cannot use the HTTP connection directly.

File and input operations return their result directly. Creation, lifecycle changes, scripts and image preparation return jobs. A VNC acknowledgement confirms that input was processed, not that an application has finished responding; inspect a screenshot or application result before the next step.
