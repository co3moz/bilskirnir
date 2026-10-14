import { fileSize } from "./uploads"
import type {
  DownloadProgress,
  EnvironmentItem,
  EnvironmentSpec,
  ImageRecord,
  Job,
  ProfileItem,
} from "./types"

export const LEGACY_IMAGE = "wsl-desktop/ubuntu:24.04"
const LEGACY_TITLE = "Ubuntu 24.04 · Lightweight XFCE (legacy)"
export const DESKTOP_HOME = "/home/desktop/Downloads"

export const memory = (bytes?: number | null) =>
  bytes ? `${(bytes / 1024 ** 3).toFixed(1)} GB` : "Unknown"
export const date = (seconds: number) => new Date(seconds * 1000).toLocaleString("en-US")
export const shortDate = (seconds: number) =>
  new Date(seconds * 1000).toLocaleString("en-US", {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  })

export function duration(seconds: number) {
  if (seconds < 60) return `${Math.max(0, Math.round(seconds))}s`
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${Math.round(seconds % 60)}s`
  return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`
}

export const stateLabels: Record<string, string> = {
  running: "Running",
  exited: "Stopped",
  created: "Starting",
  missing: "Missing",
  unavailable: "Docker unavailable",
  restarting: "Restarting",
  paused: "Paused",
  dead: "Stopped",
  queued: "Queued",
  completed: "Completed",
  failed: "Failed",
  interrupted: "Interrupted",
  ready: "Ready",
  not_ready: "Not downloaded",
  unhealthy: "Desktop error",
}

export const jobNames: Record<string, string> = {
  create: "Create environment",
  retry: "Retry setup",
  start: "Start",
  stop: "Stop",
  restart: "Restart",
  delete: "Delete environment",
  internet: "Internet",
  build: "Build image",
  "profile-build": "Build profile",
  "profile-pull": "Download profile",
  snapshot: "Snapshot",
  exec: "Run script",
  reset: "Reset environment",
  settings: "Change settings",
}

export type Tone = "success" | "warning" | "danger" | "muted" | "info"

/** What a card or row shows as the environment's state, and the color family for it. */
export function environmentStatus(item: EnvironmentItem): { key: string; tone: Tone } {
  const running = item.state === "running"
  if (item.health === "unhealthy") return { key: "unhealthy", tone: "danger" }
  if (running && item.health === "starting") return { key: "created", tone: "warning" }
  if (running) return { key: "running", tone: "success" }
  if (item.state === "missing" || item.state === "unavailable" || item.environment.last_error)
    return { key: item.state, tone: "danger" }
  if (["created", "restarting", "paused"].includes(item.state))
    return { key: item.state, tone: "warning" }
  return { key: item.state, tone: "muted" }
}

/** A stopped desktop whose setup did not finish offers a retry instead of a start. */
export const needsRetry = (item: EnvironmentItem) =>
  item.state !== "running" && (item.state === "missing" || !!item.environment.last_error)

export const needsAttention = (item: EnvironmentItem) =>
  needsRetry(item) || item.health === "unhealthy" || item.state === "unavailable"

export function jobTone(status: string): Tone {
  if (status === "completed") return "success"
  if (status === "failed" || status === "interrupted") return "danger"
  if (status === "running") return "info"
  return "warning"
}

export const jobActive = (job: Pick<Job, "status">) =>
  job.status === "queued" || job.status === "running"

/**
 * What a job worked on, by name. The manager records `environment:<id>` or `profile:<id>`; a
 * deleted environment keeps the name its deletion job reported, or a short ID.
 */
export function jobTarget(
  job: Pick<Job, "resource" | "result">,
  environments: Map<string, string>,
  profiles: Map<string, string>
) {
  if (!job.resource) return ""
  const [kind, id] = job.resource.split(/:(.*)/s)
  if (kind === "environment" && id)
    return (
      environments.get(id) ??
      (job.result?.name as string | undefined) ??
      `Environment ${id.slice(0, 8)}`
    )
  if (kind === "profile" && id) return profiles.get(id) ?? id
  return job.resource
}

export function profileTitle(
  spec: EnvironmentSpec,
  desktopTitle: string | undefined,
  profiles: ProfileItem[]
) {
  return (
    desktopTitle ||
    profiles.find((item) => item.profile.reference === spec.image)?.profile.name ||
    (spec.image === LEGACY_IMAGE ? LEGACY_TITLE : "Custom desktop")
  )
}

export function jobLogText(job: Pick<Job, "log" | "status" | "result">) {
  if (job.log?.trim()) return job.log
  if (job.status === "queued") return "Waiting to start…"
  if (job.status === "running") return "Operation in progress…"
  if (job.status === "completed") {
    if (job.result?.deleted) {
      const name = job.result.name as string | undefined
      return `${name ? `Deleted "${name}".` : "Environment deleted."}${job.result.volumes_preserved ? " Named volumes were preserved." : ""}${job.result.images_preserved ? " Saved images were preserved." : ""}`
    }
    return "Operation completed successfully. No detailed log was recorded."
  }
  return job.status === "failed"
    ? "Operation failed. No detailed log was recorded."
    : "Operation interrupted. No detailed log was recorded."
}

export type DownloadSummary = { text: string; value?: number; max?: number; percent?: number }

// Docker's per-layer counts, summarized by the manager while a job downloads an image.
export function downloadProgress(progress?: DownloadProgress | null): DownloadSummary | null {
  if (!progress) return null
  const { current = 0, total = null, layers = 0, layers_done: done = 0 } = progress
  if (!layers) return { text: "Preparing download…" }
  const counted = `${done} of ${layers} layers`
  if (!total)
    return {
      text: `${total === 0 ? "Checking layers" : `${fileSize(current)} downloaded`} · ${counted}`,
    }
  if (current >= total && done < layers)
    return { value: 1, max: 1, percent: 100, text: `Extracting · ${counted}` }
  return {
    value: current,
    max: total,
    percent: Math.floor((current * 100) / total),
    text: `${fileSize(current)} of ${fileSize(total)} · ${counted}`,
  }
}

export type ImageChoice = { value: string; label: string; disabled: boolean }

export function imageChoices(
  profiles: ProfileItem[],
  records: Pick<ImageRecord, "image_id" | "name" | "desktop" | "available">[],
  defaultImage?: string,
  selected?: string
): ImageChoice[] {
  const choices = new Map<string, ImageChoice>()
  // A missing profile downloads when it is used; one whose image does not match its labels cannot be used.
  for (const item of profiles)
    choices.set(item.profile.reference, {
      value: item.profile.reference,
      label:
        item.available || item.error ? item.profile.name : `${item.profile.name} · downloads first`,
      disabled: !item.available && !!item.error,
    })
  if (defaultImage && !choices.has(defaultImage))
    choices.set(defaultImage, {
      value: defaultImage,
      label: defaultImage === LEGACY_IMAGE ? LEGACY_TITLE : defaultImage,
      disabled: false,
    })
  for (const record of records)
    choices.set(record.image_id, {
      value: record.image_id,
      label: record.desktop ? `${record.name} · ${record.desktop.title}` : record.name,
      disabled: record.available === false,
    })
  if (selected && !choices.has(selected))
    choices.set(selected, { value: selected, label: selected, disabled: false })
  return [...choices.values()]
}

/** The choice a form starts with: the requested one when usable, otherwise the first usable one. */
export function initialImage(choices: ImageChoice[], wanted?: string) {
  return choices.some((item) => item.value === wanted && !item.disabled)
    ? wanted
    : choices.find((item) => !item.disabled)?.value
}

type TreeRecord = Pick<ImageRecord, "id" | "image_id" | "parent_image_id">

export function imageTree<T extends TreeRecord>(records: T[]): (T & { depth: number })[] {
  const ids = new Set(records.map((r) => r.image_id))
  const parents = new Map<string | null, T[]>()
  for (const record of records) {
    const parent = record.parent_image_id || null
    if (!parents.has(parent)) parents.set(parent, [])
    parents.get(parent)!.push(record)
  }
  const result: (T & { depth: number })[] = []
  const seen = new Set<string>()
  function visit(record: T, depth: number) {
    const stack: [T, number][] = [[record, depth]]
    while (stack.length) {
      const [item, level] = stack.pop()!
      if (seen.has(item.id)) continue
      seen.add(item.id)
      result.push({ ...item, depth: level })
      const children = parents.get(item.image_id) || []
      for (let i = children.length - 1; i >= 0; i--) stack.push([children[i], level + 1])
    }
  }
  for (const r of records) if (!r.parent_image_id || !ids.has(r.parent_image_id)) visit(r, 0)
  // Corrupt/cyclic metadata must not hide records or hang the UI.
  for (const r of records) if (!seen.has(r.id)) visit(r, 0)
  return result
}

// The names the settings dialog uses for settings that need a new container.
const restartNames: Record<string, string> = {
  resolution: "resolution",
  gpu: "GPU",
  audio: "audio",
  microphone: "microphone",
  sudo: "sudo",
  gamepad: "gamepad",
  shm_mb: "shared memory",
  mounts: "volumes",
  cpus: "removing the CPU limit",
  memory_mb: "removing the memory limit",
}
export function restartNotice(fields: string[], running: boolean) {
  const names = fields.map((field) => restartNames[field] || field)
  const list = names.length > 1 ? `${names.slice(0, -1).join(", ")} and ${names.at(-1)}` : names[0]
  const subject = list.charAt(0).toUpperCase() + list.slice(1)
  const effect = running
    ? "the desktop restarts and open applications close"
    : "the desktop stays stopped until you start it"
  return `${subject} ${names.length > 1 ? "need" : "needs"} a new container: ${effect}. Files in the container are kept.`
}

/** Writable places an upload can go: Downloads unless a read-only volume covers it, and writable volumes. */
export function uploadLocations(spec: Pick<EnvironmentSpec, "mounts">) {
  const covered = (mount: { target: string }) =>
    DESKTOP_HOME === mount.target || DESKTOP_HOME.startsWith(`${mount.target}/`)
  const homeReadOnly = spec.mounts.some((mount) => mount.readonly && covered(mount))
  return [
    ...(homeReadOnly ? [] : [{ target: DESKTOP_HOME, name: "Downloads" }]),
    ...spec.mounts
      .filter((mount) => !mount.readonly)
      .map((mount) => ({ target: mount.target, name: `${mount.volume} · ${mount.target}` })),
  ]
}

/** An upload subfolder stays relative to its destination. */
export function subfolderError(folder: string) {
  if (
    folder &&
    (folder.startsWith("/") ||
      folder.includes("\\") ||
      folder.split("/").some((part) => !part || part === "." || part === ".."))
  )
    return "Enter a relative subfolder without . or .. segments."
  return ""
}

export type ActivityPoint = { date: string; completed: number; failed: number }

/**
 * Finished jobs per local day over the last `days` days, oldest first. Interrupted jobs count as
 * failed: either way the operation did not finish as asked.
 */
export function jobActivity(
  jobs: Pick<Job, "status" | "created_at" | "finished_at">[],
  days: number,
  now = new Date()
): ActivityPoint[] {
  const key = (value: Date) =>
    `${value.getFullYear()}-${String(value.getMonth() + 1).padStart(2, "0")}-${String(value.getDate()).padStart(2, "0")}`
  const points = new Map<string, ActivityPoint>()
  const start = new Date(now.getFullYear(), now.getMonth(), now.getDate() - (days - 1))
  for (let i = 0; i < days; i++) {
    const day = new Date(start.getFullYear(), start.getMonth(), start.getDate() + i)
    points.set(key(day), { date: key(day), completed: 0, failed: 0 })
  }
  for (const job of jobs) {
    if (jobActive(job)) continue
    const point = points.get(key(new Date((job.finished_at ?? job.created_at) * 1000)))
    if (!point) continue
    if (job.status === "completed") point.completed++
    else point.failed++
  }
  return [...points.values()]
}

/** Sum of CPU and memory limits over running desktops; an unlimited desktop adds nothing. */
export function limitTotals(items: EnvironmentItem[]) {
  let cpus = 0
  let memoryMb = 0
  let unlimited = 0
  for (const item of items) {
    if (item.state !== "running") continue
    const { spec } = item.environment
    if (spec.cpus) cpus += spec.cpus
    if (spec.memory_mb) memoryMb += spec.memory_mb
    if (!spec.cpus || !spec.memory_mb) unlimited++
  }
  return { cpus, memoryMb, unlimited }
}
