# HTTP API

[Back to the README](../README.md) · [MCP guide](MCP.md)

The dashboard and MCP server share Bilskirnir's backend operations. You can also call the HTTP API directly from a script or another application to manage containers and build your own interface.

All paths below are relative to your installation, for example `http://localhost:8080`. Request bodies and results are JSON unless noted otherwise. Errors return an HTTP error status and an `error` message.

## Sign in

The HTTP API uses the same administrator session as the dashboard. Sign in and keep the returned cookie:

```bash
curl --cookie-jar bilskirnir.cookies \
  --header 'Content-Type: application/json' \
  --header 'X-Bilskirnir-Request: 1' \
  --data '{"username":"admin","password":"YOUR_PASSWORD"}' \
  http://localhost:8080/api/login
```

Send that cookie with subsequent requests. POST and DELETE requests also need `X-Bilskirnir-Request: 1`. If you send an Origin header, it must match the request's host and port. These request checks still apply when `AUTH_MODE=none`; only sign-in and the cookie become unnecessary.

The MCP bearer token authenticates `/mcp`, not these API routes; a [transfer link](#transfer-links) that an agent gets from MCP authorizes one file or archive transfer without either. Sessions last 12 hours; `/api/logout` revokes the session.

## Endpoints

| Method | Path | Result or purpose |
| --- | --- | --- |
| GET | `/api/session` | Sign-in mode and current authentication state; available before login. |
| POST | `/api/login` | Sign in with `username` and `password`. |
| POST | `/api/logout` | Sign out. |
| GET | `/api/settings` | Defaults and supported features, without credentials, with the manager's `version` and the `templates_url` the [templates page](#templates) reads, `null` when it is turned off, and the `volume_filter` label a volume needs to be listed, `null` when every volume is. |
| GET | `/api/system` | Docker connection and host summary. |
| GET | `/api/usage` | CPU, memory, disk and network use of every container once a second for the last 30 minutes, with the host's GPU; `since` returns only newer samples. See [resource usage](#resource-usage). |
| GET | `/api/containers` | Managed containers, state and health. |
| POST | `/api/containers` | Create a container, with values for its image's [compose](#composes) in `parameters`; returns a job. |
| GET | `/api/containers/{id}` | One container's configuration, state and health. |
| POST | `/api/containers/{id}/action` | Start, stop or restart; returns a job. |
| POST | `/api/containers/{id}/retry` | Retry partial setup or a compose setup that did not finish; returns a job. |
| POST | `/api/containers/{id}/internet` | Set internet egress; returns a job. |
| POST | `/api/containers/{id}/settings` | Change settings after creation; `dry_run` reports what recreates the container. Returns a job. |
| DELETE | `/api/containers/{id}` | Delete the container and networks; returns a job. `?delete_volumes=true` also deletes the volumes created for it. |
| POST | `/api/containers/{id}/exec` | Run a script; returns a job. |
| POST | `/api/containers/{id}/snapshot` | Save filesystem changes as an image; returns a job. |
| GET | `/api/containers/{id}/logs` | Bounded container log output. |
| GET | `/api/containers/{id}/diagnostics` | Desktop and graphics diagnostics. |
| GET | `/api/containers/{id}/usage` | One container's [resource usage](#resource-usage) samples; accepts `since`. |
| GET | `/api/containers/{id}/screenshot` | JPEG at desktop resolution; accepts `max_width`, `region` and `window_id`. |
| POST | `/api/containers/{id}/input` | Mouse/keyboard actions through VNC. |
| POST | `/api/containers/{id}/act-and-observe` | Input actions followed by a screenshot, as JSON. |
| POST | `/api/containers/{id}/screenshots` | Save a screenshot inside the desktop and return its `download_url`. |
| GET | `/api/containers/{id}/applications` | Launched applications and their state. |
| POST | `/api/containers/{id}/applications` | Launch an application in the background. |
| GET | `/api/containers/{id}/applications/{application_id}` | One application's state, pid and exit status. |
| GET | `/api/containers/{id}/applications/{application_id}/logs` | Tail of the application's log. |
| POST | `/api/containers/{id}/applications/{application_id}/stop` | Stop the application. |
| GET | `/api/containers/{id}/applications/{application_id}/diagnostics` | Process tree, resources and latest log lines. |
| GET | `/api/containers/{id}/applications/{application_id}/metrics` | CPU and memory sampled over `duration_ms`. |
| POST | `/api/containers/{id}/wait` | Wait for a window, application state, log text or a stable screen. |
| POST | `/api/containers/{id}/reset` | Recreate the container from its source image; returns a job. |
| GET | `/api/containers/{id}/recordings` | Recordings and their state. |
| POST | `/api/containers/{id}/recordings` | Start recording desktop frames. |
| POST | `/api/containers/{id}/recordings/{recording_id}/stop` | Stop a recording. |
| POST | `/api/containers/{id}/artifacts` | Package recordings and application logs as a tar.gz. |
| POST | `/api/containers/{id}/audio-capture` | Record desktop or microphone audio as WAV. |
| POST | `/api/containers/{id}/audio-inject` | Play WAV data into the microphone or speakers. |
| GET | `/api/containers/{id}/terminals` | Terminal sessions and their state. |
| POST | `/api/containers/{id}/terminals` | Open an interactive shell. |
| POST | `/api/containers/{id}/terminals/{terminal_id}/input` | Type into a terminal. |
| GET | `/api/containers/{id}/terminals/{terminal_id}/output` | Read terminal output from an offset. |
| POST | `/api/containers/{id}/terminals/{terminal_id}/close` | Hang up a terminal. |
| GET | `/api/containers/{id}/ui` | AT-SPI accessibility tree of applications. |
| POST | `/api/containers/{id}/gamepad` | Feed the virtual controller. |
| GET | `/api/containers/{id}/windows` | Visible X11 windows and the active window. |
| POST | `/api/containers/{id}/windows/{window_id}/focus` | Raise and focus a window. |
| POST | `/api/containers/{id}/windows/{window_id}/geometry` | Move or resize a window. |
| POST | `/api/containers/{id}/files/read` | Read a bounded file slice as text or base64. |
| POST | `/api/containers/{id}/files/write` | Write text or base64 file data atomically. |
| POST | `/api/containers/{id}/files/list` | List a directory without following symlinks. |
| POST | `/api/containers/{id}/files/mkdir` | Create a folder with `path` in an existing parent. |
| POST | `/api/containers/{id}/files/move` | Rename or move within the same volume. |
| POST | `/api/containers/{id}/files/delete` | Delete a file, symlink or directory. |
| GET | `/api/containers/{id}/files/download` | Stream a file with `path`; limited by `UPLOAD_MAX_MB`. |
| POST | `/api/containers/{id}/files` | Stream a binary upload. |
| GET | `/api/images` | Every [image](#images): the catalog images first, then script builds, snapshots and imports, with ancestry and availability. A script build carries `build` with its `base_image`, `internet` and, when [merged from templates](#merged-templates), its `parts` without their scripts, a compose its parameters without the scripts, an [icon](#image-icons) its version and an image built from templates their ids in `template_ids`. |
| POST | `/api/images` | Import a desktop-compatible local image using `name` and `reference`. |
| DELETE | `/api/images/{id}` | [Delete an image](#delete-an-image): a saved image's record with its Docker image, or a catalog image's download. `id` is the list's `id`. Answers at once with `{"id", "name", "deleted": true, "image_removed"}`; `409` while a container was created from it. |
| POST | `/api/images/build` | Build an image using `name`, `base_image`, `script` or [`parts`](#merged-templates) and optional `description`, `internet`, `compose`, `icon` and `template_id` or `template_ids`; returns a job. |
| POST | `/api/images/{id}/pull` | Download a catalog image, or a newer release of it; `id` is its catalog ID. A release manager downloads the one published with it. Returns a job. |
| POST | `/api/images/{id}/build` | Build a catalog image from the bundled Docker context instead; returns a job. |
| GET | `/api/images/{id}/layers` | What an image's [disk space](#image-sizes-and-layers) is made of: its sizes and the steps of its history, newest first, each with its size, its instruction and the image it came from. `id` is the list's `id`; `404` when Docker does not have the image. |
| GET | `/api/images/{id}/config` | The `name`, `base_image`, `internet` and `script` or `parts` a script build ran, with the image's `description`, `compose`, `icon` and `template_id`, or `template_ids` for several, when it has them, a body `/api/images/build` accepts as it is. `id` is the record's `id`, not its Docker `image_id`. `404` for snapshots, imports and builds from earlier releases. |
| GET, PUT, DELETE | `/api/images/{id}/compose` | Read, set or remove an image's [compose](#composes). `id` is the record's `id`. |
| GET, PUT, DELETE | `/api/images/{id}/icon` | Read, set or remove a saved image's [icon](#image-icons). |
| GET | `/api/images/{id}/archive` | Download a saved image as a gzip-compressed `docker save` archive; the transfer is recorded as a job. |
| POST | `/api/images/import` | Stream a `.tar.gz` or `.tar` image archive with optional `size`, `name` and `description`; returns a job. |
| GET | `/api/volumes` | Docker named volumes that carry the `VOLUME_FILTER` label (by default the ones Bilskirnir created), each as `docker volume ls` describes it plus a boolean `manager` and an `owner`. A route or container setting that names a volume the filter leaves out answers as if it did not exist, except for a volume a container already mounts. `manager: true` marks the manager's own volumes, such as its data volume: containers cannot mount them and their files cannot be opened. Without `MANAGER_CONTAINER`, or when it names no manager container, none is marked. `owner` is set on a volume a [`new_volume` parameter](#composes) created for a container, from its labels: `{"container_id": ..., "parameter": "WORKSPACE", "name": "Unity dev", "deleted": false}`, with `name: null` and `deleted: true` once that container's record is gone. It is `null` on every other volume. |
| POST | `/api/volumes` | Create a named volume using `name`, with the manager's label and the one `VOLUME_FILTER` asks for. |
| DELETE | `/api/volumes/{name}` | Delete a named volume and its files at once, without a job; returns `{"name": ..., "deleted": true}`. `403` for the manager's own volumes, `404` for a missing one or one `VOLUME_FILTER` leaves out, and `409` while a container record mounts it, through its volume settings or its compose, with that container's name in `error`, or while Docker reports another container using it. Nothing is forced or pruned. |
| POST | `/api/volumes/{name}/files/list` | List a folder of a volume; see [volume files](#volume-files). |
| POST | `/api/volumes/{name}/files/read` | Read a bounded file slice as text or base64. |
| POST | `/api/volumes/{name}/files/write` | Write text or base64 file data atomically. |
| POST | `/api/volumes/{name}/files/mkdir` | Create a folder. |
| POST | `/api/volumes/{name}/files/move` | Rename or move within the volume. |
| POST | `/api/volumes/{name}/files/delete` | Delete a file, symlink or folder. |
| GET | `/api/volumes/{name}/files/download` | Stream a file with `path`; limited by `UPLOAD_MAX_MB`. |
| POST | `/api/volumes/{name}/files` | Stream a binary upload. |
| GET | `/api/volumes/{name}/archive` | Download a volume's files as a `.tar.gz`; the transfer is recorded as a job. |
| POST | `/api/volumes/import` | Stream a `.tar.gz` or `.tar` with `name` and optional `size` into a new volume; returns a job. |
| POST | `/api/volumes/{name}/extract` | Stream a `.tar.gz` or `.tar` with `path` and optional `size` into a new folder of the volume; returns a job. |
| POST | `/api/volumes/{name}/copy` | Copy the volume's files into a new volume named by `name`; returns a job. |
| GET, PUT, POST | `/api/transfers/{token}` | Run the one upload, download, import or export a [transfer link](#transfer-links) authorizes; needs no session. |
| Any | `/api/containers/{id}/ports/{port}/{path}` | HTTP or WebSocket to a [port inside the desktop](#desktop-ports) that its `ports` setting lists; needs the MCP token, not a session. |
| GET | `/api/jobs` | Latest 100 job summaries. |
| GET | `/api/jobs/{job_id}` | Job status, log and result. |
| GET | `/healthz` | Basic manager health; does not require sign-in. |

The VNC and media WebSocket routes are `/api/containers/{id}/vnc` and `/api/containers/{id}/audio/{direction}`, where direction is `output` or `input`. They use the browser session and require a matching Origin. VNC uses the binary subprotocol. Most automation clients can use screenshots and `/input` instead of implementing a VNC viewer.

## Create a container and follow the job

```bash
curl --cookie bilskirnir.cookies \
  --header 'Content-Type: application/json' \
  --header 'X-Bilskirnir-Request: 1' \
  --data '{"name":"Agent desktop","image":"ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04","gpu":true,"internet":true}' \
  http://localhost:8080/api/containers
```

The response contains `job_id`. Poll its detail endpoint:

```bash
curl --cookie bilskirnir.cookies http://localhost:8080/api/jobs/JOB_ID
```

A job is finished when its status is `completed`, `failed` or `interrupted`. A successful creation puts `container_id` in `result`. Read the log for a failed job; after interruption, inspect the resources before retrying.

`GET /api/containers` returns each container's record, with `id`, `spec` (its creation settings), `container` (the Docker container's name), `source_image_id` and `last_error`, and Docker's view beside those fields: `state` is Docker's container state, `missing` when the container is gone or `unavailable` when Docker cannot be reached; `health` is `healthy`, `unhealthy`, `starting` or `unknown`; `status` is Docker's status text. `GET /api/containers/{id}` returns one item of the same shape.

Creating a container or building an image from a catalog image that is not downloaded yet pulls it first, in the same job. While the download runs, the job carries `progress`: `current` and `total` compressed bytes, where `total` is `null` until the size of every remaining layer is known, and `layers_done` of `layers`. Layers already on the host count as done and are not part of the byte total. The field is removed when the download ends. A download that fails is tried up to three times in the same job, unless Docker refused it, such as for an unknown image.

A script build (`POST /api/images/build`) also lists its `stages`, in order, once it starts: `base` prepares the base image, downloading it if needed; `script` runs the setup script; `layer` saves the layer the script wrote; `image` writes and tags the image; and `cleanup` removes the build's temporary files and tag. Each stage is `{"name", "started_at", "finished_at"}` in Unix seconds, and a stage that has not started carries only its name. The job log has a line such as `Stage 3/5: Saving the image layer` when one starts. Stages only move forward: a stage that never started before a later one was skipped, and in a failed or interrupted job the last stage that started is where it stopped. `progress` belongs to the running stage and only `base` has one, while its download runs. Docker reports no measurable progress while the script runs or while it saves the layer, so those stages have only their times; saving a large layer can take minutes. The `layer` and `image` stages start from BuildKit's own progress lines; with a builder that does not print them, `layer` is skipped and its time counts toward `script`.

A snapshot (`POST /api/containers/{id}/snapshot`) and a settings change that recreates its container ([below](#change-settings-after-creation)) list their stages the same way. A snapshot has `check`, which confirms the container is the one the manager created; `commit`, which commits the container layer to an image while Docker briefly pauses the desktop; and `record`, which saves the image record. A recreating settings change has `stop`, skipped when the container was not running; `commit`; `create`, which creates the new container and starts it if the old one ran; and `cleanup`, which removes the previous container and the image an earlier settings change committed. A settings change that applies in place has no stages. `docker commit` reports nothing until it is done, so `commit` has only its times, and committing a large container layer can take minutes. Other jobs have no `stages`.

Creation accepts `name`, `image` and these optional fields:

| Field | Default | Meaning |
| --- | --- | --- |
| `resolution` | `1920x1080` | Desktop size, from 320x240 to 7680x4320. |
| `gpu` | `false` | Request GPU access supported by the installation. |
| `internet` | `true` | Attach the Docker egress network. |
| `audio`, `microphone` | `false` | Enable supported browser media; microphone also needs audio. |
| `sudo` | `true` | Allow the desktop user to use sudo. |
| `gamepad` | `false` | Attach `/dev/uinput` so the gamepad route can create a virtual controller. |
| `hostname` | The Docker container name | The desktop's [machine identity](#machine-identity): one label of letters, digits and hyphens, at most 63 characters. |
| `mac_address` | Random | A unicast MAC address such as `02:42:ac:11:00:02`. Capitals and hyphens are accepted and stored as lowercase with colons. |
| `machine_id` | Random | 32 hex digits, written to `/etc/machine-id` and `/var/lib/dbus/machine-id`. A UUID with hyphens is accepted. |
| `ports` | `[]` | Ports inside the desktop that the [port proxy](#desktop-ports) forwards to, at most 32, from 1 to 65535. |
| `cpus`, `memory_mb` | No limit | Positive CPU count and at least 256 MiB of memory. |
| `shm_mb` | `512` | Shared memory size, at least 64 MiB. |
| `mounts` | `[]` | Objects with `volume`, absolute `target` and optional `readonly`. The manager's own volumes, such as its data volume, are refused, here and whenever the container is created again. |
| `parameters` | `{}` | Values for the [compose](#composes) parameters of the image, by name, or as `STEP.NAME` for a [compose of several steps](#steps). |

### Machine identity

`hostname`, `mac_address` and `machine_id` are what software inside the desktop, such as a license bound to one machine, sees of the machine it runs on. Docker gives every container it creates a new hostname and new MAC addresses, so the manager keeps all three in the container's `spec` and gives them to every Docker container it creates for it: a reset, a retry and a settings change that recreates the container keep them. A new container gets its own, also one started from a snapshot of another container. Leave them out unless one container is meant to take over the identity of another it replaces.

- The hostname is passed to `docker create`; `/etc/hostname` and `/etc/hosts` follow it.
- The MAC address is the private network's address from creation. The internet network gets the same address when it is connected, so whichever interface software reads, it sees one address; Docker numbers a container's interfaces in no fixed order. Only the Engine API takes an address for a network connected after creation, and Docker Engine 29 (API 1.54) is the first release that uses it. Earlier releases, and an installation whose `DOCKER_HOST` is not a Unix socket, give the internet interface an address of their own at every start.
- The machine ID is written to `/etc/machine-id` and `/var/lib/dbus/machine-id` in the created container before it starts. The catalog images carry one ID that every container of a release shares, and a snapshot carries the ID of the container it was taken from, so neither is used as it is.

Records saved before identities were kept have `null` for all three until a Docker container is next created for them. They then take the hostname of the container being replaced, and its MAC address if it is running, since Docker gives a stopped container its address only when it starts; without such a container they get new ones. Their machine ID stays `null`, which leaves the ID their image or commit carries, as before, until the settings set one.

## Change settings after creation

`POST .../settings` takes the creation fields to change, except `image`; fields that are left out keep their value. `name`, `internet`, `ports` and a new or changed `cpus` or `memory_mb` apply to the running container. `resolution`, `gpu`, `audio`, `microphone`, `sudo`, `gamepad`, `hostname`, `mac_address`, `machine_id`, `shm_mb`, `mounts` and removing a limit (`null`) cannot be changed on an existing Docker container: the job stops the container, commits its container layer to an image tagged `bilskirnir/container:JOB_ID`, creates a new container from that image with the new settings and starts it again if it was running. Running applications close; files in the container layer are kept, and named volumes are not touched. If the new container cannot start, the previous one is restored. A reset still recreates the container from its original source image, and deleting the container removes the committed image unless a snapshot builds on it.

With `"dry_run": true` the request is validated and answered at once with `live` and `restart`, the changed fields grouped by how they apply, without a job:

```bash
curl --cookie bilskirnir.cookies \n  --header 'Content-Type: application/json' \n  --header 'X-Bilskirnir-Request: 1' \n  --data '{"audio":true,"dry_run":true}' \n  http://localhost:8080/api/containers/CONTAINER_ID/settings
```

A feature the installation has switched off cannot be turned on, and an identity field can be changed but not set back to `null`. The job's result repeats the `live` and `restart` lists.

Start/stop/restart uses `{"action":"start"}`, internet uses `{"enabled":false}` and snapshot uses `{"name":"Editor installed","description":"Optional note"}`. Actions on a container are serialized. Deleting it preserves named volumes and saved images, but removes unsaved changes in its container layer. Its job result is `{"deleted": ID, "name": ..., "volumes_preserved": true, "images_preserved": true}`.

`DELETE /api/containers/{id}?delete_volumes=true` also deletes the volumes that [`new_volume` parameters](#composes) created for this container: those whose labels name it as their owner, found by label and inspected again before anything is removed, never a volume chosen by name or the manager's own. A volume another container mounts, including one that picked it by name, stays, and so does one Docker refuses to remove because something still uses it; the job log says why. The volumes are removed after the container itself, so a volume the job could not remove does not keep the container. The result then adds `volumes_deleted` and `volumes_kept`, the names of the volumes created for the container that were removed and those that stay, and `volumes_preserved` is `false` once one was removed. The option is off by default; the dashboard's delete dialog offers it, unchecked, when the container has such volumes.

## Run a script

POST to `/api/containers/{id}/exec`:

```json
{
  "script": "apt-get update\napt-get install -y git",
  "root": true,
  "timeout_seconds": 600
}
```

The default user is UID/GID 1001:1001; root execution must be explicit. Bash runs with fail-on-error and with standard input closed, so a command that reads input receives end-of-file instead of consuming the rest of the script. The response is a job, with output and exit failures in its log. Timeouts range from 1 to 3500 seconds, defaulting to 300. The working directory is `/home/desktop`; the container's network and resource limits apply. The script holds the container's operation lock until it finishes, so start graphical programs with the [applications API](#applications-and-windows) instead of a script. See [command execution](MCP.md#run-commands-and-install-software) for more detail.

## Images

`GET /api/images` lists every image a container can start from, each with a `kind`:

| Kind | Image |
| --- | --- |
| `catalog` | A desktop published with Bilskirnir. It is downloaded the first time a container or a build starts from it. |
| `build` | Built from a base image with a script through `/api/images/build`. |
| `snapshot` | A container's files saved with `.../snapshot`. |
| `import` | A desktop image already on the Docker host, registered with `POST /api/images`. |
| `archive` | Loaded from an [image archive](#image-archives). |

The catalog images come first. Their `id` is the catalog ID, such as `ubuntu-xfce`, and a container or a build names one by its `reference`. `image_id` is `null` until the image is downloaded, `downloaded` tells whether Docker has the reference and `available` whether it can be used; `error` explains a downloaded reference whose labels belong to another desktop. `created_at`, `parent_image_id` and `source_container` are `null`. The other kinds are saved records, named by their Docker `image_id`, with `available: false` once Docker no longer has the image. Every kind reports `desktop`, the catalog image it derives from (`catalog_id`) and its session, when its labels say so.

### Delete an image

`DELETE /api/images/{id}` deletes an image, where `id` is the list's `id`. It cannot be undone. What goes depends on the kind:

| Kind | What goes |
| --- | --- |
| `build`, `snapshot`, `archive` | The record, and Docker's image with `docker image rm` of the tag Bilskirnir gave it, or of its `image_id` once that tag is gone. Docker removes only the tag while another tag or another image still holds the image. |
| `import` | Only the record. The image was on the Docker host before it was registered, so Docker keeps it. |
| `catalog` | Only the download. The image stays listed with `downloaded: false`, and a container, a build or `.../pull` downloads it again. `404` when it is not downloaded. |

The answer comes at once, without a job: `{"id": ..., "name": ..., "deleted": true, "image_removed": true}`, where `image_removed` tells whether Docker deleted the image itself rather than only its tag. Nothing is forced and nothing is pruned, and a refusal deletes nothing. A container created from the image refuses it with `409` and its name, since a reset, a retry and a settings change that recreates the container need that image again; delete the container first. Another saved image of the same Docker image, such as an archive imported on the host it was exported from, keeps it for the container, so either record can go then. Docker's own refusal, for an image another Docker container still uses or one that a tag Bilskirnir did not give it still names, is `409` with Docker's message. So is a delete while a download, a build or another change of the same image runs. Images built or saved from the deleted one keep working: Docker keeps the layers they share with it.

## Image sizes and layers

Every image Docker has carries three sizes in bytes, all `null` for an image Docker does not have:

| Field | Size |
| --- | --- |
| `size` | The size Docker reports, as `docker image ls` shows it. The containerd image store, the default of Docker 29 and of the WSL package, also counts the compressed copy it keeps of each layer. |
| `layers_size` | What the image's layers hold unpacked. |
| `own_size` | The part of `layers_size` that no other listed image brought. Images share the layers they have in common, and Docker stores a shared layer once, so own sizes add up to the space the listed images take without counting a layer twice. |

`GET /api/images/{id}/layers` explains them with the image's history, the newest step first as `docker history` lists it:

```json
{
  "id": "5b1e0c9f2a7d4e3b8c6a1f0e9d8c7b6a",
  "image_id": "sha256:c86959cb711a...",
  "size": 2630695169,
  "layers_size": 1977327616,
  "own_size": 1067704320,
  "steps": [
    {"instruction": "LABEL bilskirnir.managed=true", "kind": "config", "size": 0, "created": 1792351597, "layer": false, "from": null, "shared_with": []},
    {"instruction": "RUN bash /tmp/bilskirnir-setup.sh && rm /tmp/bilskirnir-setup.sh", "kind": "run", "size": 1067692032, "created": 1792351597, "layer": true, "from": null, "shared_with": []},
    {"instruction": "ADD file:7121eb4a5ba3... in /", "kind": "add", "size": 87633920, "created": 1790928654, "layer": true, "from": "ubuntu-xfce", "shared_with": ["ubuntu-xfce", "0d4c2b1a9e8f7d6c5b4a3f2e1d0c9b8a"]}
  ]
}
```

`instruction` is the step as a Dockerfile says it, without the shell and build arguments the builder recorded around it, and at most 4 KiB. `kind` is `run`, `copy` or `add` for steps that write files, `config` for settings such as `ENV` or `LABEL`, which write none, and `commit` for a container's files saved by `docker commit`: a snapshot, or a settings change that carried a container's files over to its replacement, named in `comment`. `size` is the bytes the step wrote, unpacked, `created` is in Unix seconds (`null` when Docker records no time), and `layer` tells whether the step wrote one of the image's layers. `from` is the list `id` of the image the step came from, the listed image with the fewest layers that has it, or `null` when it is this image's own; `shared_with` names every other listed image that has it. Steps are compared by the layers below them rather than by their history, so an image built on an earlier build of a catalog image still shares the layers that build has in common with the current one. Docker never changes an image ID's layers or history, so the manager reads them once per image.

## Image icons

A saved image can have an icon, a PNG of at most 256 by 256 pixels and 300 KiB that lists show beside its name. Requests and exported configurations carry it as a data URL, `"icon": "data:image/png;base64,..."`; the dashboard scales any picture to fit before it sends one. `PUT /api/images/{id}/icon` with `{"icon": "data:image/png;base64,..."}` sets it and `DELETE` removes it, both returning the image as lists show it. Lists name the icon by a version instead of carrying it, and `GET /api/images/{id}/icon?v=<version>` returns the PNG; with the current version the response may be cached for good, and without it the browser revalidates with the `ETag`. A snapshot keeps the icon of the saved image its container was created from, and image archives carry it. Catalog images have none.

## Templates

The dashboard's **Templates** page lists ready-made images. It reads the list in the browser from `templates_url` in `/api/settings`, by default `templates/index.json` on the project's GitHub wiki; `TEMPLATES_URL` points it elsewhere or turns it off. A template is a build request, the same JSON `/api/images/{id}/config` returns, beside the list as `<id>.json` with its icon as `<id>.png`; installing one fills in the **Build image** form, and the build sets `template_id` so the page can tell which templates are installed. The manager itself never fetches a template, and the content security policy allows the list's host for these requests and pictures only. [The development guide](DEVELOPMENT.md#add-a-template) describes the list format.

### Merged templates

**Merge and install** builds several templates into one image, such as Unity development with Google Chrome. The page merges them in the browser into one build request with `parts` instead of a `script`, a compose with one [step](#steps) for every template that has one, and their ids in `template_ids`, in the order they were chosen:

```json
{
  "name": "Unity development + Google Chrome",
  "base_image": "ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04",
  "internet": true,
  "parts": [
    {"id": "unity-dev", "name": "Unity development", "script": "#!/usr/bin/env bash\n..."},
    {"id": "chrome", "name": "Google Chrome", "script": "#!/usr/bin/env bash\n..."}
  ],
  "compose": {"steps": [{"id": "unity-dev", "name": "Unity development", "parameters": [], "script": "..."}]},
  "template_ids": ["unity-dev", "chrome"]
}
```

A request has a `script` or 1 to 8 `parts`, not both. Each part has an `id` of lowercase letters, digits and dashes, unique in the build, a `name` of one line and at most 100 characters, and a `script`; the scripts take at most 1,000,000 bytes together. The build runs them as root in this order, each in a layer of its own named after its part, so the build's output says whose script failed and Docker's cache keeps the parts before it. The image is built on one base, the first template's; the page warns when another template was made for a different one. The build form shows every part before anything runs, and a part can be moved or left out there, with its compose step. An exported configuration of a merged image has the same `parts` and `template_ids`, so it builds the image again elsewhere, and can be a template of its own.

## Composes

A saved image can carry a compose: parameters that a new container from it asks for, and a setup script that receives them. `PUT /api/images/{id}/compose` sets or replaces it, `GET` returns it with its script and `DELETE` removes it; `id` is the record's `id`. Image lists and job results show the compose without its script.

```json
{
  "parameters": [
    {"name": "UNITY_EMAIL", "label": "Unity email", "description": "The Unity ID you sign in with.",
     "required": true},
    {"name": "UNITY_PASSWORD", "label": "Unity password", "required": true, "secret": true},
    {"name": "ACCELERATOR", "label": "Cache server", "pattern": "[A-Za-z0-9.-]+:[0-9]{1,5}",
     "pattern_message": "host:port, for example 192.168.1.20:10080"},
    {"name": "UNITY_EDITOR", "label": "Editor", "type": "choice", "allow_custom": true,
     "options": [{"value": "6000.0.23f1", "label": "Unity 6"}, {"value": "2022.3.50f1", "label": "2022 LTS"}],
     "default": "6000.0.23f1"},
    {"name": "BUILD_JOBS", "label": "Build jobs", "type": "number", "display": "slider",
     "min": 1, "max": 16, "default": "4"},
    {"name": "PLATFORMS", "label": "Platforms", "type": "choices",
     "options": [{"value": "linux"}, {"value": "windows"}, {"value": "webgl"}], "default": "linux"},
    {"name": "UNITY_HOME", "label": "Unity installs", "type": "volume", "path": "/opt/unity",
     "default": "unity-installs"},
    {"name": "WORKSPACE", "label": "Workspace", "type": "volume", "path": "/home/desktop/Projects",
     "new_volume": true},
    {"name": "UNITY_PROJECT", "label": "Unity project", "type": "folder",
     "default": "/home/desktop/UnityProjects/MyGame"}
  ],
  "script": "set -eu\nwhile read -r platform; do echo \"$UNITY_EDITOR for $platform\"; done <<< \"$PLATFORMS\"\ntest -d \"$UNITY_HOME/$UNITY_EDITOR\" || echo \"install into $UNITY_HOME\"\n",
  "root": false,
  "timeout_seconds": 600
}
```

| Field | Default | Meaning |
| --- | --- | --- |
| `parameters` | `[]` | Up to 32 objects with `name` and optional `label`, `description`, `type`, `display`, `required`, `default` and the fields of their type, below. |
| `script` | Required | Bash script of at most 1,000,000 bytes, run with fail-on-error and standard input closed. |
| `root` | `false` | Run as root instead of the desktop user, UID/GID 1001:1001. |
| `timeout_seconds` | `600` | From 1 to 3500 seconds. |

### Steps

A compose can also be several steps, run in order, such as one for each template [merged](#merged-templates) into an image. It is written as `{"steps": [...]}`, 1 to 8 objects with the fields above and an `id` of lowercase letters, digits and dashes, unique in the compose, and an optional `name` the dashboard shows. A compose of one step without an `id` or `name` is written with its fields at the top, as above. The scripts take at most 1,000,000 bytes together, and each step has its own `root` and `timeout_seconds`.

Each step's script receives only its own parameters, so two steps may ask for the same name, such as a `VERSION` each. A value then goes by `STEP.NAME`: the create request's `parameters`, the container's `setup.parameters` and `setup.secrets` all name it `chrome.VERSION` or `vscode.VERSION`. A request may still give a value by its name alone when only one step asks for that name; when several do, the request is refused with the names to use. Volume parameters of all steps still need paths of their own. A step's mistake, in the compose or a value, names the step, and the dashboard asks for each step's values in a section of its own.

The setup runs the steps one after another in the same job. A step that fails stops the setup, and the error names it; a retry or a reset runs every step again, so write each script to work where it already ran. The setup job's log masks the secrets of every step in each step's output, in case a script prints something an earlier one saved.

A parameter's `name` is the environment variable that carries its value to the script: at most 64 capital letters, digits and underscores, starting with a letter. Names that the shell or the desktop session relies on, such as `PATH`, `HOME`, `DISPLAY` or names starting with `LD_`, `XDG_` or `DESKTOP_`, are refused. A `label` is what the dashboard asks for, the name when empty. A `description` of at most 500 characters is shown with the field, such as where to find the value. Values are text of at most 16 KiB. A secret parameter cannot have a default, because image lists return the compose.

Every value reaches the script as text, so a parameter's `type` decides what text it accepts and how the dashboard asks for it. `default` is written the same way the script receives it.

| `type` | `display` | The script receives | Fields of the type |
| --- | --- | --- | --- |
| `text` (default) | `input` (default), `multiline` | The text; one line unless `multiline` | `secret`, `pattern` and `pattern_message` for a format |
| `number` | `input` (default), `slider` | A plain decimal such as `42` or `-1.5`, without exponents | `min`, `max`, `step`; a slider needs `min` and `max`, and steps count from `min` or from 0 |
| `boolean` | `toggle` (default), `checkbox` | `true` or `false`; `false` when left out | |
| `choice` | `select` (default), `radio` | One option's `value` | `options`, `allow_custom` to also accept other text |
| `choices` | `checkboxes` | The chosen values, one per line, each at most once | `options` |
| `volume` | `select` | Its `path`, and the chosen volume's name in `NAME_VOLUME` | `path`, required; `new_volume` for a volume of each container's own |
| `folder` | `input` | An absolute folder of the desktop | |

`options` holds 1 to 100 objects with a `value` of one line and an optional `label` the dashboard shows instead. A field of another type, a display the type does not have or a default the parameter would refuse is an error. `required` means a value must be given; for a boolean, that it must be `true`, as for an agreement. Read several choices in bash with `while read -r item; do ...; done <<< "$PLATFORMS"`.

A text parameter's `pattern` is a regular expression that a value must match as a whole, as if it were written `^(?:pattern)$`: one line of at most 500 characters in the syntax of Rust's [regex crate](https://docs.rs/regex/latest/regex/#syntax). A multiline value is matched as a whole too, so a pattern for several lines spells out the line breaks, such as `[a-z]+(\n[a-z]+)*`. The crate takes time linear in the value whatever the pattern, and has no lookaround or backreferences. A pattern it cannot compile, or one that needs more than 4 MiB once compiled, such as `\w{1,100}` with its Unicode classes, is refused when the compose is saved, with an error that names the parameter. `pattern_message` describes the expected format in one line of at most 200 characters, and needs a `pattern`: the dashboard shows it under the field, and the error repeats it, `Cache server does not have the expected format: host:port, for example 192.168.1.20:10080`. An empty value is still left to `required`, a `default` must match, and a secret can have a pattern too; the error never contains the value. The dashboard checks a pattern as it is typed, with the browser's regular expressions, which read most patterns the same way. There, `\d`, `\w` and `\b` cover ASCII only, while in the manager they cover the digits and letters of every script, so write `[0-9]` or `[A-Za-z]` where the difference matters. A pattern the browser cannot read, such as one with `\A` or `(?i)`, is checked by the manager alone when the container is created.

A volume parameter keeps data that every container of an image needs in the same place, such as Unity installs, out of the image and the container. Its value is the name of an existing named volume (`GET /api/volumes`, one not marked `manager`), or empty or `"-"` for none, and `required` means a volume must be chosen. The volume is mounted at the parameter's `path`, an absolute, normalized folder that the compose fixes and that follows the same rules as a [volume setting](#create-a-container-and-follow-the-job): not `/`, `/opt`, `/var` or `/home` themselves, and nothing under the system folders or the paths the manager mounts itself, such as `/tmp`, `/run`, `/dev` and `/usr`. Two volume parameters cannot share a path, and no other parameter can be named like a volume parameter's second variable. The script receives the path in the parameter's variable and the volume's name, or an empty string, in the same name with `_VOLUME` appended: `UNITY_HOME=/opt/unity` and `UNITY_HOME_VOLUME=unity-installs`. Without a volume the folder stays inside the container, in its files and its snapshots. Either way, before the script runs, a missing folder is created and an empty one owned by root is given to the desktop user, as Docker leaves a new volume's mount point to root; a folder that already holds files keeps its owner. Docker creates the missing folders above a volume's path owned by root; those inside a folder of the desktop user, such as its home, go to the desktop user too while they hold nothing else.

A volume parameter with `"new_volume": true` gives each container a volume of its own instead, such as a workspace that no other container shares. A container whose request leaves the value empty, or out, gets the volume `bilskirnir-CONTAINER_ID-NAME`, the parameter's name in lowercase with dashes for underscores, such as `bilskirnir-0f1e...e1f0-workspace`, or `bilskirnir-CONTAINER_ID-STEP-NAME` for a step of [several](#steps), whose step id and parameter name take at most 83 characters together; a request that names a volume still uses that one, and one that gives `"-"` gets none, which the container keeps as an empty value, as the dashboard's **No volume** does. `required` then refuses only `"-"`, and the parameter cannot have a `default`, since an empty value is what asks for the new volume. The creation job creates the volume, so a request that is refused or a form that is cancelled leaves nothing behind, with the labels `bilskirnir.managed=true`, `bilskirnir.environment=CONTAINER_ID` and `bilskirnir.parameter=NAME`. A volume of that name that does not carry the container's labels stops the job. The value, the script's `NAME_VOLUME` and the container's `setup.mounts` name the volume like a chosen one, and a reset, a retry and a settings change mount it again. A container created from a snapshot gets a new volume of its own, not its source container's. The volume outlives its container unless the container is deleted with `delete_volumes=true`, and `GET /api/volumes` names its owner.

A folder parameter names a folder of the desktop the script works in, such as the project an editor opens. Its value is an absolute, normalized path outside the system folders, by the rules of a volume parameter's `path` except that it may hold a comma: `/home/desktop/UnityProjects/MyGame` or `/srv/projects`, not `/tmp/build`. The manager neither creates the folder nor changes its owner; the script does what it needs. Uploads may go into it and into a volume parameter's `path`, with or without a volume, and the dashboard's upload dialog offers both by their labels, as [files](#files) describes.

`POST /api/containers` takes the values in `parameters`, such as `{"UNITY_EMAIL":"dev@example.com","UNITY_PASSWORD":"..."}`. A value that is left out uses the default. Every parameter reaches the script, an optional one without a value as an empty string, so `set -u` does not trip on it. An unknown name, a missing required value or a value its parameter refuses, such as text that does not match its `pattern`, fails the request, and so does a volume parameter's volume that is also a `mounts` entry's volume, chosen for two volume parameters, or given at a path a `mounts` entry already takes. The job refuses a volume that does not exist, without creating an empty one, and the manager's own volumes, which a reset, a retry and a settings change that recreates the container refuse too. The one exception is the volume a `new_volume` parameter gets, which the job creates before it saves the container record. Once the record exists, its volume was created, so a reset, a retry or a settings change that finds it missing stops like it does for any other volume instead of creating an empty one in its place: the volume may have held the container's work, and an empty replacement would hide that it is gone. Restore it under the same name, for example by [importing an archive](#volume-archives) of it, or delete the container. The creation job waits until the desktop reports ready, for at most five minutes, then runs the script in the desktop session's environment (`HOME`, `DISPLAY`, `XDG_RUNTIME_DIR`) and completes only when the script succeeds.

The container pins the compose and the values, so editing the image later does not change what it runs. Its `setup` field reports `status` (`pending`, `running`, `completed` or `failed`), the `image` record the compose came from, the values of ordinary `parameters`, the names of the `secrets`, the `mounts` its volume parameters were given, as `volume` and `target`, and the `places` its volume and folder parameters name, as `label` and `path`; a folder parameter left empty names none. Containers created before places were kept have none. A compose of [several steps](#steps) adds `steps`, each step's `id`, `name` and `status` in order, and its places' labels begin with the step's name. Those volumes are mounted beside the `mounts` setting every time the container is created again: by a reset, a settings change and retry. The settings do not list them, so a settings request cannot drop them, and one that puts another volume at their path or their volume at another path is refused. Secret values are never returned, and the setup job's log shows `***` in their place. The manager keeps them in its database until the container is deleted, so that the setup can run again: after a reset, and when retry recreates a missing container from the source image. A settings change keeps the container's files and does not run it. A script that fails leaves the container running with `last_error` set; retry runs it again.

A snapshot keeps the compose its container was created with, so a container created from the snapshot asks for the values again. Write the script so it also works in a container where it already ran. The build configuration and an image archive carry the compose too.

The job stops reading the script's output a second after the script exits. A program the script leaves running fails if it writes to that output later, so start long-running programs with the [applications API](#applications-and-windows), or redirect their output.

## Files

File reads and writes use the desktop user's permissions and require a running container. They reject symlinks and noncanonical paths. Reads accept regular files only.

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

`POST .../files/list` with `{"path":"/home/desktop/project","limit":500}` returns `items` with `name`, `type` (`file`, `directory`, `symlink` or `special`), `size`, `modified_at` and `mode`, plus `total` and `truncated`. `POST .../files/mkdir` with `{"path":"/home/desktop/project/assets"}` creates one folder in an existing parent and returns 409 when the name exists. `POST .../files/move` with `{"source":...,"destination":...,"replace":false}` renames within the same volume and returns 409 when the destination exists. `POST .../files/delete` with `{"path":...,"recursive":false}` removes a file, a symlink itself or a directory; top-level directories, `/home/desktop` and mounted volume roots are refused. `GET .../files/download?path=...` streams a regular file with `Content-Length`, a guessed `Content-Type` and an attachment `Content-Disposition`; it refuses symlinks and files larger than `UPLOAD_MAX_MB`. Responses that describe a saved file, such as `download_file`, `export_artifacts` and saved screenshots, include this route as a root-relative `download_url` for the dashboard session.

For larger uploads, POST the raw file body with `Content-Type: application/octet-stream` to `/api/containers/{id}/files`. Supply `destination`, `name`, `size` and optional `replace` as query parameters:

```bash
curl --cookie bilskirnir.cookies \
  --header 'X-Bilskirnir-Request: 1' \
  --header 'Content-Type: application/octet-stream' \
  --data-binary @notes.txt \
  'http://localhost:8080/api/containers/CONTAINER_ID/files?destination=/home/desktop/Downloads&name=notes.txt&size=FILE_SIZE_IN_BYTES'
```

Replace `FILE_SIZE_IN_BYTES` with the exact size. URL-encode filenames and destination paths when necessary. Streaming uploads are limited by `UPLOAD_MAX_MB`. They return the saved path and byte count, and create missing folders of the destination.

Uploads, saved screenshots and exported artifacts follow the same destination rule. The folder may be anywhere in the desktop home, `/home/desktop`, in a volume mounted writable, by the `mounts` setting or a compose's volume parameter, or in one of the `places` in the container's `setup`; nothing under a read-only volume is accepted. The rule keeps writes out of the system folders. What protects the desktop is that files are written as the desktop user, so its permissions apply, and that no symlink in the destination is followed.

## Volume files

The `/api/volumes/{name}/files/...` routes work on a named volume without a container that mounts it, with the same request bodies and limits as a container's [file routes](#files). Paths are absolute within the volume: `/` is its root, which can be listed and written into but not deleted or moved. `mkdir` takes `{"path":"/project/assets"}` and returns 409 when the name exists. Uploads take `destination`, `name`, `size` and optional `replace` as query parameters, as a container's do.

Each request starts a short-lived helper container from the manager's own image, so it needs `MANAGER_CONTAINER`. The helper mounts only that volume, has no network, a read-only root and no capabilities beyond the ones the operation needs, never gets the Docker socket and is removed when the request ends. Files are read and written as the desktop user, UID/GID 1001, so the permissions a desktop would meet apply: a `403` names a file that user may not change. Docker creates a volume's root owned by root; a write to a volume that is still empty first gives its root to the desktop user, as mounting it over a desktop folder would. A volume that holds files keeps its owners.

A missing volume returns 404 instead of being created. The manager's own volumes, such as its data volume, are refused with 403. Writes to one volume run one at a time; reads do not wait for them.

## Volume archives

`GET /api/volumes/{name}/archive` streams a volume's files as an ordinary gzip-compressed tar, so other tools read it too. A helper container reads the volume as root on a read-only mount, with only the capability to read every file, so files the desktop user may not read are included; numeric owners and permissions are recorded. The transfer is a `volume-export` job, named in the `X-Job-Id` response header, whose `progress` counts the bytes sent. A file that changes while it is read is archived as it was read and noted in the job log.

```bash
curl --cookie bilskirnir.cookies --output unity-projects.tar.gz \
  http://localhost:8080/api/volumes/unity-projects/archive
```

`POST /api/volumes/import?name=NEW_VOLUME` with `Content-Type: application/octet-stream` and the archive as the body creates the volume `name` and unpacks the archive into it. The name must not exist yet: an import never replaces a volume's files, and a name in use returns 409. The archive is `.tar.gz` or `.tar`, at most `IMAGE_ARCHIVE_MAX_MB` after decompression. tar unpacks it as the desktop user in a helper without network, so every file belongs to UID/GID 1001: a setuid bit grants nothing beyond that user, device files cannot be created, and nothing lands outside the volume. The response is a `volume-import` job once the whole archive has arrived; if the upload or unpacking fails, the new volume is removed again.

`size`, the archive's exact length in bytes, is optional. With it, a body of any other length fails the import, and a size above the limit is refused at once with 413. Without it, an archive can stream in as it is made, with no temporary file on the sending side:

```bash
tar -c -C project . | curl --cookie bilskirnir.cookies \
  --header 'X-Bilskirnir-Request: 1' \
  --header 'Content-Type: application/octet-stream' \
  --request POST --upload-file - \
  'http://localhost:8080/api/volumes/import?name=NEW_VOLUME'
```

curl sends such a body in chunks. Its bytes are counted as they arrive, and the import fails once they pass `IMAGE_ARCHIVE_MAX_MB`. A connection that closes before the last chunk fails it too, and so does an archive that ends without tar's end-of-archive blocks, which every tar writes: GNU tar would otherwise unpack an archive cut at a file boundary as if it were whole, as one is when the sending tar stops early. The job's `progress` then has `current` and no `total`.

`POST /api/volumes/{name}/extract?path=/FOLDER` unpacks a `.tar.gz` or `.tar` into a new folder of a volume that exists, such as one a running container mounts. The body, `size` and limits are those of an import:

```bash
tar -c -C project . | curl --cookie bilskirnir.cookies \
  --header 'X-Bilskirnir-Request: 1' \
  --header 'Content-Type: application/octet-stream' \
  --request POST --upload-file - \
  'http://localhost:8080/api/volumes/unity-projects/extract?path=/worktree-42'
```

The folder must not exist yet. The extract creates it, so no file of the volume can be replaced and no symlink already in the volume can lead the archive elsewhere. `path` is absolute and canonical, as for the [volume file routes](#volume-files), and its parent must exist; `/` is the volume itself, accepted only while it is empty, such as right after `POST /api/volumes`. tar runs inside the new folder as the desktop user, in a helper that mounts only that volume, so every file belongs to UID/GID 1001. The extract holds the volume's write lock, so file writes to the volume wait for it. A folder that exists returns 409, a missing volume 404 without creating it, and the manager's own volumes 403. The response is a `volume-extract` job once the whole archive has arrived; if the upload or unpacking fails, the folder is removed again, or the volume emptied again for `/`. Unpacking over a folder that already holds files is not supported: extract into a new folder, then delete or move the old one. An MCP client, which has no session, gets a `volume_extract` [transfer link](#transfer-links) for this route.

`POST /api/volumes/{name}/copy` with `{"name":"NEW_VOLUME"}` copies every file of the volume into a new one, for work that starts from a prepared volume or a copy kept before a risky change; MCP clients call `copy_volume`. Docker has no volume copy, so the manager pipes the two archive helpers into each other, without compression: one reads the source as root on a read-only mount with only the capability to read every file, the other unpacks into the new volume as the desktop user, and no helper mounts both. The copy keeps the tree, symlinks, hard links and modes. Every file belongs to UID/GID 1001, as after an import, and device files cannot be copied. The new name must not exist (409), a missing source returns 404 and is not created, and the manager's own volumes are refused on either side with 403. The response is a `volume-copy` job whose `progress` counts the bytes copied. It takes one of the two archive transfer slots, `IMAGE_ARCHIVE_MAX_MB` does not apply since nothing is uploaded, and a copy that fails removes the new volume again. A file that changes while it is read is copied as it was read and noted in the job log.

A copy is a full copy, not copy-on-write: disk use grows by the volume's size, and the WSL virtual disk does not shrink by itself after a volume is deleted.

## Image archives

`GET /api/images/{id}/archive` streams a saved image, where `id` is the record's `id` from `GET /api/images`. The download is a gzip-compressed `docker save` archive of the image ID, so it names no image. Its first entry, `bilskirnir.json`, holds the record's name, description, kind, desktop information, icon and template, and a script build's configuration, so `/api/images/{id}/config` works for the imported record too. Docker's own `docker load` reads the archive too and reports the loaded image ID.

```bash
curl --cookie bilskirnir.cookies --output unity.tar.gz \
  http://localhost:8080/api/images/IMAGE_RECORD_ID/archive
```

An archive holds every layer, the base desktop included, and its size is not known in advance. Docker prepares the whole image before its first layer arrives. The transfer is an `image-export` job, named in the `X-Job-Id` response header, whose `progress` counts the bytes sent. If Docker fails partway, the download ends with an error instead of a short file.

`POST /api/images/import` takes the archive as an `application/octet-stream` body: an exported `.tar.gz` or a plain `docker save` `.tar`. Supply its exact `size` when it is known, and optionally `name` and `description`, as query parameters; the name and description default to the values stored in the archive. `--upload-file` streams the file instead of reading it into memory:

```bash
curl --cookie bilskirnir.cookies \
  --header 'X-Bilskirnir-Request: 1' \
  --header 'Content-Type: application/octet-stream' \
  --request POST --upload-file unity.tar.gz \
  'http://localhost:8080/api/images/import?size=FILE_SIZE_IN_BYTES&name=Unity%20workspace'
```

The archive streams into `docker load` as it arrives. On the way, the manager removes every tag and image name the archive carries, so an import cannot replace a catalog image or any other existing tag. Docker can register the image only after the archive has passed these checks:

- It holds exactly one image, with its `manifest.json` and configuration.
- The image has the `bilskirnir.desktop=true` label.
- It is at most `IMAGE_ARCHIVE_MAX_MB` after decompression.

The response arrives once the whole archive has passed: `{"job_id":...}` while Docker finishes loading in that `image-import` job, or status 400 with the reason it was refused. A `size` above the limit is refused at once with 413, and a third transfer while two archives are moving with 429. Without `size`, as for `docker save IMAGE_ID | curl ... --upload-file -`, the archive is held to the limit as it arrives, the job's `progress` has no `total`, and an archive cut short is refused because it lacks the manifest, the configuration or a layer that loading needs. The finished job's result is the new image record, of kind `archive`, with the reference `bilskirnir/imported:<id>`.

## Transfer links

`/api/transfers/{token}` runs one of the transfer routes above for a client that has neither a session nor the administrator password: an agent that holds the MCP token gets the link from the MCP tool `create_transfer`, which names the operation and its arguments. The [MCP guide](MCP.md#move-large-files) lists the operations: uploading into a container or volume, downloading from one, importing or exporting a volume or image archive, and unpacking an archive into a new folder of a volume.

The request runs the route the link stands for, with that route's limits, locks, job and answer, and the arguments the link was issued with. Uploads and imports take `PUT` or `POST` with the file or archive as the body, whatever its `Content-Type`; their size is `Content-Length`, or a `size` query parameter when the body is sent without one. A file upload is refused with 411 when neither is there, while an import then takes the archive as one of unknown size, as described under [volume archives](#volume-archives). Downloads and exports take `GET`. No other query parameter is accepted.

```bash
curl --fail-with-body --upload-file unity-projects.tar.gz http://localhost:8080/api/transfers/TOKEN
```

The token replaces the cookie and the `X-Bilskirnir-Request` header; an `Origin` header, if present, must still match the host. It is 244 random bits, held in memory for five minutes, and spent by the first request that presents it, whatever the outcome: a method the operation does not take answers 405, and a second use, an expired link or one the manager forgot in a restart answers 404. Signing out does not end a transfer a link began. Without MCP the route answers 404. The token is left out of job logs, error messages and request tracing.

## Desktop ports

A desktop publishes no ports. The manager reaches it on the private network it shares with it, and `/api/containers/{id}/ports/{port}/{path}` forwards HTTP and WebSocket requests there, to a port the container's `ports` setting lists. A program in the desktop that serves its own API, such as an editor's MCP server, a test runner or a debugger, is then usable without a script job per request. `ports` is empty by default; it is set at creation or changed later, and a change applies at once because the manager, not Docker, does the forwarding. The service must listen on the desktop's network, such as on `0.0.0.0`: one that listens on `127.0.0.1` only is out of the manager's reach.

```bash
curl --header 'Authorization: Bearer YOUR_MCP_TOKEN' \
  http://localhost:8080/api/containers/CONTAINER_ID/ports/8090/status
```

The service receives the method, `{path}` with the query, the body and the other headers as sent, with `Host: localhost:PORT`, as a local client would. Responses stream as the service sends them, server-sent events included, and only the connection to the service has a time limit of 10 seconds. A WebSocket handshake that the service accepts joins the two connections until either side closes. A port not listed answers 403, a stopped container 409, and a service that does not answer 502.

Whatever a desktop serves would appear on the dashboard's address, and a desktop browses sites nobody vouches for, so the route keeps the dashboard's session away from it:

- It takes the MCP bearer token, as `/mcp` does, and never the session cookie, so a browser does not open it signed in. With MCP disabled it answers 404; with `MCP_AUTH_MODE=none` it needs no token, like `/mcp`. A request with an `Origin` header must come from the manager's own address.
- The `Authorization` header, the `bilskirnir_session` cookie, `Origin` and the connection's own headers are removed before the request is forwarded, so the desktop sees no credential of the manager. Other cookies pass. A service that requires its own `Authorization` header cannot be reached this way.
- Every response carries `Content-Security-Policy: sandbox` beside the dashboard's own policy, and `X-Content-Type-Options: nosniff`. A browser that renders one, as it could with `MCP_AUTH_MODE=none`, runs no script in it and treats it as an origin of its own. `Set-Cookie` and `Clear-Site-Data` from the service are dropped, and a redirect to a path of the service stays under the proxy's prefix.

## Screenshots

`GET /api/containers/{id}/screenshot` returns a JPEG at the desktop's resolution, so its pixels match `/input` coordinates. `max_width` (16 to 7680) scales the image down, `region=x,y,width,height` crops in desktop pixels and `window_id` captures one window from the windows route. The response carries `X-Image-Width`, `X-Image-Height` and `X-Captured-At` (Unix milliseconds) headers. The dashboard requests `max_width=640` for previews.

`POST /api/containers/{id}/screenshots` with `{"path":"/home/desktop/Downloads/clock.jpg","replace":false,"capture":{"max_width":960}}` captures with the same options and writes the JPEG inside the desktop as the desktop user, under the upload destination rules. It returns `path`, `bytes`, `width`, `height`, `captured_at` and `download_url`; an existing file returns 409 unless `replace` is set.

`POST /api/containers/{id}/act-and-observe` takes `{"actions":[...],"settle_ms":500,"capture":{"max_width":960}}`, performs the actions, waits `settle_ms` (0 to 10000) and returns `{"input":{...},"screenshot":{"width","height","captured_at","bytes","jpeg_base64"}}`.

## Mouse and keyboard

POST to `/api/containers/{id}/input` with an `actions` array:

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

POST `{"command":"glxgears","args":["-info"],"cwd":"/home/desktop","gpu":true,"env":{"MY_SETTING":"1"},"name":"gears"}` to `/api/containers/{id}/applications`. The program runs in the background as the desktop user; `gpu` runs it through `gpu-run` and needs a GPU-enabled container. The response includes `application_id`, `state`, `pid` and, once the program ends, `exit_code` or the terminating `signal`. States are `starting`, `running`, `exited`, `failed` and `unknown`.

`GET .../applications/{application_id}/logs?tail_bytes=65536` returns the end of the combined stdout/stderr log, which keeps about the latest 1 MiB. `POST .../applications/{application_id}/stop` with an optional `{"grace_seconds":5}` body sends SIGTERM to the process group, then SIGKILL. Supervisor files live under `/tmp` in the desktop and disappear when it restarts, after which the state is `unknown`.

`GET .../applications/{application_id}/diagnostics` returns the application's state, exit status, its session's processes with memory use, open file count, thread count and the latest log lines. `GET .../applications/{application_id}/metrics?duration_ms=1000` samples CPU time and resident memory of those processes over the window (100–10000 ms) and reports container memory from cgroups; frame rate is not available.

`POST .../wait` with `{"window_title":"Game","application_id":"...","application_state":"running","log_contains":"ready","screen_stable_ms":1000,"region":"0,0,800,600","timeout_ms":10000,"interval_ms":500}` polls until every supplied condition holds and returns `satisfied`, `elapsed_ms` and the last observation; after `timeout_ms` (100–60000) it returns `satisfied:false` rather than an error. `application_state` defaults to `exited`, which also matches `failed`.

`POST .../reset` recreates the container from its saved source image ID and returns a job. Unsaved changes in the container layer are discarded; named volumes, networks and the container ID are kept, and the data inside volumes is not touched. The reset is refused when the source image is missing.

`POST .../recordings` with `{"fps":2,"max_seconds":60,"max_width":960,"region":"0,0,1280,720","name":"level 1"}` starts capturing JPEG frames into `/tmp/bilskirnir-recordings/{recording_id}` inside the desktop and returns `recording_id`. Recording stops at `max_seconds`, after 3000 frames or 256 MiB, or on `POST .../recordings/{recording_id}/stop`, which returns `frames`, `bytes`, `started_at`, `ended_at` and `reason`. Frames are named `frame-00000-<unix ms>.jpg` and can be read with the file routes; stopping also writes `recording.avi`, an MJPEG video playable with VLC, mpv or ffmpeg, whose details are returned under `video`. `POST .../artifacts` with `{"destination":"/home/desktop/Downloads","name":"run.tar.gz","recording_ids":[...],"application_ids":[...],"replace":false}` writes an archive containing `recordings/<id>/` frames, `applications/<id>/` logs and a `manifest.json`; the destination follows the upload rules and existing archives return 409 unless `replace` is set. Recording files live in tmpfs and disappear when the desktop restarts.

## Audio, terminals, accessibility and gamepad

`POST .../audio-capture` with `{"seconds":3,"source":"desktop"}` records what the desktop plays (or, with `microphone`, what reaches its microphone) as 16-bit stereo 48 kHz WAV and returns `wav_base64`, `peak`, `rms` and `silent`. `POST .../audio-inject` with `{"data_base64":"...","target":"microphone"}` plays 16-bit PCM WAV data into the desktop's microphone (needs the microphone option) or, with `speakers`, into its audio output. Both need audio enabled for the container.

`POST .../terminals` with an optional `{"cwd":"/home/desktop","root":false}` opens bash on a pseudo-terminal and returns `terminal_id`. `POST .../terminals/{terminal_id}/input` with `{"text":"ls","press_enter":true}` types into it; `GET .../terminals/{terminal_id}/output?offset=0&limit=65536&strip_ansi=true` returns `output`, `next_offset`, `more` and the shell state, keeping about the latest 1 MiB. `POST .../terminals/{terminal_id}/close` hangs up the shell. Root shells are an explicit administrator choice.

`GET .../ui?pid=1234&name=&max_depth=6&max_nodes=500` returns the AT-SPI accessibility tree of applications that expose one (GTK, Electron, Qt with `QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1`): roles, names, states, screen extents, text and actions. Games usually expose nothing.

`POST .../gamepad` with `{"actions":[{"type":"button","button":"a"},{"type":"axis","axis":"left_x","value":0.5},{"type":"wait","milliseconds":100},{"type":"reset"}]}` drives a virtual Xbox-style controller in a container created with `gamepad:true`. The device is created on first use as `/dev/input/eventN` inside the desktop and keeps button and axis state between requests until `reset` or a desktop restart. Buttons: a, b, x, y, lb, rb, back, start, guide, ls, rs. Axes: left_x, left_y, right_x, right_y (-1 to 1), left_trigger, right_trigger (0 to 1), dpad_x, dpad_y (-1, 0, 1). SDL applications that use udev can set `SDL_JOYSTICK_DISABLE_UDEV=1` to scan `/dev/input` directly.

`GET .../windows` lists visible windows with `window_id`, `title`, `pid`, `x`, `y`, `width`, `height` and `active`. `POST .../windows/{window_id}/focus` activates a window, and `POST .../windows/{window_id}/geometry` with `x`,`y` and/or `width`,`height` moves or resizes it and returns the resulting geometry. Window IDs are hexadecimal X11 IDs such as `0x3a00007`. The window routes, window screenshots and relative pointer movement need `xdotool` and `wmctrl` in the desktop image.

## Resource usage

The manager follows the Docker stats stream of every running container and keeps one sample a second for the last 30 minutes. The history lives in memory only: a manager restart starts it over, and stopped and deleted containers keep their samples until they are 30 minutes old. It needs the Docker Engine socket; when `DOCKER_HOST` names a TCP address instead, there is no history.

`GET /api/usage` returns:

- `available` and `error`: whether Docker's running containers can be read right now. The samples already taken stay while it cannot.
- `interval_ms` (1000) and `window_ms` (1800000).
- `cursor`: the stamp of the newest sample. Pass it back as `since` to receive only the samples stamped after it; two requests in a row miss nothing.
- `containers`: each container that has samples, with `id`, `name`, `cpu_limit` (its CPU limit, or `null`), `removed`, `memory_limit` (bytes, the host's memory when it has no limit), `online_cpus` (the host CPUs it sees) and `samples`. A container stays listed with an empty `samples` while nothing new arrived, so a client can drop the ones that are no longer listed. A deleted container stays listed with `removed: true`, under the name and CPU limit it last had, until its samples age out; it comes after the existing ones.
- `gpu`: the host's GPU, below.

Each sample has `t`, the Unix time in milliseconds when it was stored, and:

| Field | Meaning |
| --- | --- |
| `cpu` | CPU cores in use: the CPU % of `docker stats` divided by 100. It is measured against the host's CPUs, not the container's limit, so a container limited to two CPUs reads at most 2. |
| `memory` | Bytes in use without the page cache, as `docker stats` counts them. |
| `disk_read`, `disk_write` | Bytes per second through the container's block I/O counters. The desktop's tmpfs folders and shared memory are not disk I/O. |
| `net_rx`, `net_tx` | Bytes per second received and sent on all of the container's networks. |

`gpu.state` is `sampling`, `idle` while no GPU container runs, `unavailable` with a `reason`, or `disabled` when the installation gives desktops no GPU (`WSL_GPU` and `NVIDIA_COMPUTE` are false). Each of `gpu.devices` has `index`, `name`, `memory_total` and samples of `t`, `utilization` (percent) and `memory` (bytes); a figure the driver does not report is `null`. These are the whole GPU's figures, which `scope: "host"` says: WSL gives Linux a paravirtual GPU whose driver reports neither processes nor containers, and Windows applications count too. They come from `nvidia-smi`, so only NVIDIA GPUs report them. While a GPU container runs, the manager runs it in a helper container from its own image that gets the GPU device, no network, no capabilities and a read-only root, and ends with the manager.

`GET /api/containers/{id}/usage` returns the same header with one container's fields and samples, and no GPU. Its `samples` stay empty until the container has run since the manager started. A deleted container's ID is still answered while its samples are in the window, and returns 404 once they have aged out.

## Limits and results

JSON request bodies are limited to 1,100,000 bytes. Binary uploads have their own configured limit. Logs retain the last 256 KiB; application logs keep about 1 MiB. Screenshot requests share two processing slots, uploads allow two simultaneous transfers, archive exports, imports, extracts and volume copies two more, and desktop input allows four; operations on one container still use its resource lock, and a launched application does not hold it while it runs.

MCP JSON responses are limited to 8 MiB and, like every `/mcp` response, use chunked transfer encoding, which survives the [HTTP scanning of some Windows security products](MCP.md#broken-responses-from-localhost-on-windows). This includes screenshots returned as base64 image content. A [stdio bridge](MCP.md#connect-through-stdio-on-windows) is bundled for clients that cannot use the HTTP connection directly.

File and input operations return their result directly. Creation, lifecycle changes, scripts and image preparation return jobs. A VNC acknowledgement confirms that input was processed, not that an application has finished responding; inspect a screenshot or application result before the next step.
