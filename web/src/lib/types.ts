// The JSON the manager's HTTP API returns; see docs/API.md.

export type Mount = { volume: string; target: string; readonly: boolean }

export type EnvironmentSpec = {
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

export type DesktopInfo = { profile: string; title: string; session: string }

export type Environment = {
  id: string
  container: string
  source_image_id: string
  created_at: number
  spec: EnvironmentSpec
  last_error?: string | null
  desktop?: DesktopInfo | null
}

export type Health = "healthy" | "unhealthy" | "starting" | "unknown"

export type EnvironmentItem = {
  environment: Environment
  /** Docker's container state, `missing` without a container or `unavailable` without Docker. */
  state: string
  health: Health
  status: string | null
}

export type Profile = {
  id: string
  name: string
  reference: string
  session: string
  description: string
}

export type ProfileItem = {
  profile: Profile
  available: boolean
  image_id: string | null
  error?: string | null
}

export type ImageRecord = {
  id: string
  name: string
  image_id: string
  reference: string
  parent_image_id: string | null
  source_environment: string | null
  kind: string
  created_at: number
  description: string
  desktop?: DesktopInfo | null
  /** Recorded by script builds; `GET images/{id}/config` adds the script. */
  build?: Omit<BuildConfig, "name" | "script"> | null
  /** `null` when Docker could not list images. */
  available?: boolean | null
}

/** A build request, which is also what an image's exported configuration holds. */
export type BuildConfig = {
  name: string
  base_image: string
  internet: boolean
  script: string
}

/** One line of `docker volume ls --format json`. */
export type Volume = {
  Name: string
  Driver: string
  Scope: string
  Labels?: string
  Mountpoint?: string
}

export type JobStatus = "queued" | "running" | "completed" | "failed" | "interrupted"

export type DownloadProgress = {
  current?: number
  total?: number | null
  layers?: number
  layers_done?: number
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

export type SettingsPreview = { environment_id: string; live: string[]; restart: string[] }
