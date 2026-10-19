// The JSON the manager's HTTP API returns; see docs/API.md.

export type Mount = { volume: string; target: string; readonly: boolean }

export type ContainerSpec = {
  name: string
  image: string
  resolution: string
  gpu: boolean
  internet: boolean
  audio: boolean
  microphone: boolean
  sudo: boolean
  gamepad: boolean
  /** The machine identity; `null` on containers created before identities were kept. */
  hostname?: string | null
  mac_address?: string | null
  machine_id?: string | null
  /** Ports inside the desktop that the manager proxies for holders of the MCP token. */
  ports: number[]
  cpus: number | null
  memory_mb: number | null
  shm_mb: number
  mounts: Mount[]
}

/** What an image's labels say about its desktop; `catalog_id` names the catalog image it derives from. */
export type DesktopInfo = { catalog_id: string; title: string; session: string }

export type Container = {
  id: string
  /** The Docker container's name. */
  container: string
  source_image_id: string
  created_at: number
  spec: ContainerSpec
  last_error?: string | null
  desktop?: DesktopInfo | null
  /** Set when the image had a compose: its setup's progress and the values it was given. */
  setup?: ContainerSetup | null
}

/** A compose parameter: an environment variable of the setup script. */
export type ParameterType =
  "text" | "number" | "boolean" | "choice" | "choices" | "volume" | "folder"

export type ParameterDisplay =
  "input" | "multiline" | "slider" | "toggle" | "checkbox" | "select" | "radio" | "checkboxes"

export type ParameterOption = { value: string; label: string }

/**
 * A compose parameter: an environment variable of the setup script. Whatever its type, its value
 * and default are text in the form the script reads: `42`, `true`, or a choice per line. A volume
 * parameter's value names a volume, or is empty for none; the script reads its `path`. A folder
 * parameter's value is an absolute folder of the desktop, which uploads can go into.
 */
export type ComposeParameter = {
  name: string
  label: string
  /** Help shown with the field, such as where to find the value. */
  description: string
  type: ParameterType
  /** The type's first display when left out. */
  display?: ParameterDisplay | null
  /** A boolean that is required must be turned on. */
  required: boolean
  secret: boolean
  default: string
  options?: ParameterOption[]
  allow_custom?: boolean
  min?: number | null
  max?: number | null
  step?: number | null
  /** Volume parameters: where the volume is mounted in the desktop. */
  path?: string
  /** Text parameters: a regular expression, in Rust's syntax, a value must match as a whole. */
  pattern?: string
  /** Text parameters with a pattern: the expected format, shown when a value does not match. */
  pattern_message?: string
  /** Volume parameters without a default: an empty value gets the container a volume of its own. */
  new_volume?: boolean
}

/** What a new container from an image asks for, and the script that receives it. */
export type Compose = {
  parameters: ComposeParameter[]
  script: string
  root: boolean
  timeout_seconds: number
}

export type ContainerSetup = {
  status: "pending" | "running" | "completed" | "failed"
  /** The image record the compose came from. */
  image: string
  /** Values of the parameters that are not secret. */
  parameters: Record<string, string>
  /** Names of the secret parameters, whose values are never returned. */
  secrets: string[]
  finished_at?: number | null
  /** The volumes its volume parameters were given, mounted beside the settings' own. */
  mounts?: Mount[]
  /** The folders its volume and folder parameters name, which take uploads. */
  places?: Place[]
}

/** A folder of the desktop that a compose names, by its parameter's label. */
export type Place = { label: string; path: string }

export type Health = "healthy" | "unhealthy" | "starting" | "unknown"

/** A container record with Docker's view of it. */
export type ContainerItem = Container & {
  /** Docker's container state, `missing` without a container or `unavailable` without Docker. */
  state: string
  health: Health
  status: string | null
}

/**
 * An image as `GET images` lists it: the catalog images published with Bilskirnir first, then the
 * saved ones. `kind` is `catalog`, `build`, `snapshot`, `import` or `archive`.
 */
export type ImageItem = {
  /** A catalog image's catalog ID, such as `ubuntu-xfce`, or a saved image's record ID. */
  id: string
  name: string
  /** Docker's image ID; `null` for a catalog image that is not downloaded. */
  image_id: string | null
  reference: string
  parent_image_id: string | null
  source_container: string | null
  kind: string
  /** `null` for a catalog image. */
  created_at: number | null
  description: string
  desktop?: DesktopInfo | null
  /** Recorded by script builds; `GET images/{id}/config` adds the script. */
  build?: Omit<BuildConfig, "name" | "script"> | null
  /** Lists leave out the setup script; `GET images/{id}/compose` returns it. */
  compose?: Omit<Compose, "script"> | null
  /**
   * Whether a container can start from it now, `null` when Docker could not list images. A
   * catalog image that is not downloaded is not available, but downloads when it is used.
   */
  available?: boolean | null
  /** Catalog images: whether Docker has the reference, even one whose labels do not match. */
  downloaded?: boolean
  /** Catalog images: why a downloaded reference cannot be used. */
  error?: string | null
  /** Saved images with an icon: its version; `GET images/{id}/icon` returns the PNG. */
  icon?: string | null
  /** The templates page template the image was built from. */
  template_id?: string | null
  /** Bytes as Docker reports them; `null` when Docker does not have the image. */
  size?: number | null
  /** What the image's layers hold unpacked. */
  layers_size?: number | null
  /** The part of `layers_size` no other listed image brought. */
  own_size?: number | null
}

/** One step of an image's history, as `GET images/{id}/layers` lists it. */
export type ImageStep = {
  /** The step as a Dockerfile says it; empty for a commit that recorded no command. */
  instruction: string
  kind: "run" | "copy" | "add" | "config" | "commit"
  /** Bytes the step wrote, unpacked. */
  size: number
  /** Unix seconds. */
  created: number | null
  /** A commit's message, such as the snapshot it saved. */
  comment?: string
  /** Whether the step wrote one of the image's layers. */
  layer: boolean
  /** The list ID of the image the step came from; `null` for the image's own. */
  from: string | null
  /** The list IDs of the other images that have the step. */
  shared_with: string[]
}

/** What an image's disk space is made of; its steps are the newest first. */
export type ImageLayers = {
  id: string
  image_id: string
  size: number
  layers_size: number
  own_size: number
  steps: ImageStep[]
}

/**
 * A build request, which is also what an image's exported configuration holds and what a
 * template on the templates page is.
 */
export type BuildConfig = {
  name: string
  description?: string
  base_image: string
  internet: boolean
  script: string
  compose?: Compose
  /** A PNG of at most 256 by 256 pixels as a `data:image/png;base64,` URL. */
  icon?: string
  template_id?: string
}

/** One line of `docker volume ls --format json`, and whether it is the manager's own. */
export type Volume = {
  Name: string
  Driver: string
  Scope: string
  Labels?: string
  Mountpoint?: string
  /** One of the manager's own volumes, such as its data volume: never mounted or opened. */
  manager?: boolean
  /** The container a compose created this volume for; `null` for any other volume. */
  owner?: VolumeOwner | null
}

export type VolumeOwner = {
  container_id: string
  /** The compose parameter the volume was created for. */
  parameter: string | null
  /** The container's name, or `null` once it was deleted. */
  name: string | null
  deleted: boolean
}

export type JobStatus = "queued" | "running" | "completed" | "failed" | "interrupted"

export type DownloadProgress = {
  current?: number
  total?: number | null
  layers?: number
  layers_done?: number
  /** Set for an archive transfer or a volume copy, whose progress counts bytes instead of layers. */
  transfer?: "export" | "import" | "copy"
}

/** A named part of a job, such as a script build's layer save; times are the manager's seconds. */
export type JobStage = {
  name: string
  started_at?: number | null
  finished_at?: number | null
}

export type Job = {
  id: string
  kind: string
  resource?: string | null
  status: JobStatus
  created_at: number
  finished_at: number | null
  log: string
  result?: Record<string, unknown> | null
  progress?: DownloadProgress | null
  /** Every stage of a job that reports them, in order; one that has not started has no times. */
  stages?: JobStage[]
}

export type Settings = {
  /** The manager's release, such as `0.4.0`. */
  version: string
  base_image: string
  audio_enabled: boolean
  microphone_enabled: boolean
  gpu_enabled: boolean
  wsl_gpu: boolean
  nvidia_compute: boolean
  gpu_adapter?: string | null
  upload_limit_bytes: number
  /** The largest unpacked image archive an import accepts. */
  archive_limit_bytes: number
  /** The template list the templates page reads, or `null` when the page is turned off. */
  templates_url: string | null
  /** The label a volume needs to be listed and mounted, or `null` when every volume is. */
  volume_filter: string | null
}

export type SystemInfo = {
  connected: boolean
  version?: string
  cpus?: number
  memory_bytes?: number
  containers?: number
  error?: string
}

export type Session = { auth_enabled: boolean; authenticated: boolean }

export type JobStarted = { job_id: string }

export type SettingsPreview = { container_id: string; live: string[]; restart: string[] }
