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
export type ParameterType = "text" | "number" | "boolean" | "choice" | "choices" | "volume"

export type ParameterDisplay =
  "input" | "multiline" | "slider" | "toggle" | "checkbox" | "select" | "radio" | "checkboxes"

export type ParameterOption = { value: string; label: string }

/**
 * A compose parameter: an environment variable of the setup script. Whatever its type, its value
 * and default are text in the form the script reads: `42`, `true`, or a choice per line. A volume
 * parameter's value names a volume, or is empty for none; the script reads its `path`.
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
}

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
}

/** A build request, which is also what an image's exported configuration holds. */
export type BuildConfig = {
  name: string
  base_image: string
  internet: boolean
  script: string
  compose?: Compose
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
}

export type JobStatus = "queued" | "running" | "completed" | "failed" | "interrupted"

export type DownloadProgress = {
  current?: number
  total?: number | null
  layers?: number
  layers_done?: number
  /** Set for an image archive, whose progress counts bytes instead of layers. */
  transfer?: "export" | "import"
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
