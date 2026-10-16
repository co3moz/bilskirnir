import { formatDate, formatList, formatNumber, getLocale, has, plural, t } from "./i18n"
import { fileSize } from "./uploads"
import type { DownloadProgress, ContainerItem, ContainerSpec, ImageItem, Job } from "./types"

export const LEGACY_IMAGE = "wsl-desktop/ubuntu:24.04"
const legacyTitle = () => t("images.legacyTitle")
export const DESKTOP_HOME = "/home/desktop/Downloads"

const oneDecimal = { minimumFractionDigits: 1, maximumFractionDigits: 1 }

export const memory = (bytes?: number | null) =>
  bytes
    ? t("units.gigabytes", { value: formatNumber(bytes / 1024 ** 3, oneDecimal) })
    : t("common.unknown")
export const date = (seconds: number) => formatDate(seconds)
export const shortDate = (seconds: number) =>
  formatDate(seconds, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  })

export function duration(seconds: number) {
  if (seconds < 60) return t("duration.seconds", { seconds: Math.max(0, Math.round(seconds)) })
  if (seconds < 3600)
    return t("duration.minutes", {
      minutes: Math.floor(seconds / 60),
      seconds: Math.round(seconds % 60),
    })
  return t("duration.hours", {
    hours: Math.floor(seconds / 3600),
    minutes: Math.floor((seconds % 3600) / 60),
  })
}

/** A container, image or job state in words; an unknown one as Docker named it. */
export function stateLabel(state: string) {
  const key = `state.${state}`
  return has(key) ? t(key) : state
}

// The kinds these jobs had before catalog images were listed with the others.
const renamedJobs: Record<string, string> = {
  "profile-pull": "image-pull",
  "profile-build": "source-build",
}

/** The name of a job kind; one this release does not know as the manager recorded it. */
export function jobName(kind: string) {
  const key = `job.${renamedJobs[kind] ?? kind}`
  return has(key) ? t(key) : kind
}

export type Tone = "success" | "warning" | "danger" | "muted" | "info"

/** What a card or row shows as the container's state, and the color family for it. */
export function containerStatus(item: ContainerItem): { key: string; tone: Tone } {
  const running = item.state === "running"
  if (item.health === "unhealthy") return { key: "unhealthy", tone: "danger" }
  if (running && item.health === "starting") return { key: "created", tone: "warning" }
  // A compose setup that is waiting with an error recorded did not get to run.
  const setup = item.setup?.status
  const failed = setup === "failed" || (setup === "pending" && !!item.last_error)
  if (running && failed) return { key: "setup_failed", tone: "danger" }
  if (running && (setup === "pending" || setup === "running"))
    return { key: "setting_up", tone: "info" }
  if (running) return { key: "running", tone: "success" }
  if (item.state === "missing" || item.state === "unavailable" || item.last_error)
    return { key: item.state, tone: "danger" }
  if (["created", "restarting", "paused"].includes(item.state))
    return { key: item.state, tone: "warning" }
  return { key: item.state, tone: "muted" }
}

/** A stopped container whose setup did not finish offers a retry instead of a start. */
export const needsRetry = (item: ContainerItem) =>
  item.state !== "running" && (item.state === "missing" || !!item.last_error)

export const needsAttention = (item: ContainerItem) =>
  needsRetry(item) ||
  item.health === "unhealthy" ||
  item.state === "unavailable" ||
  containerStatus(item).key === "setup_failed"

export function jobTone(status: string): Tone {
  if (status === "completed") return "success"
  if (status === "failed" || status === "interrupted") return "danger"
  if (status === "running") return "info"
  return "warning"
}

export const jobActive = (job: Pick<Job, "status">) =>
  job.status === "queued" || job.status === "running"

/**
 * What a job worked on, by name. The manager records `container:<id>` or `image:<id>`, where a
 * catalog image's ID is its catalog ID; a deleted container keeps the name its deletion job
 * reported, or a short ID.
 */
export function jobTarget(
  job: Pick<Job, "resource" | "result">,
  containers: Map<string, string>,
  images = new Map<string, string>()
) {
  if (!job.resource) return ""
  const [kind, id] = job.resource.split(/:(.*)/s)
  const reported = job.result?.name as string | undefined
  // Jobs from before environments were called containers name them `environment:<id>`.
  if ((kind === "container" || kind === "environment") && id)
    return containers.get(id) ?? reported ?? t("jobs.target.container", { id: id.slice(0, 8) })
  // Downloads recorded before catalog images were listed with the others name `profile:<id>`.
  if (kind === "profile" && id) return images.get(id) ?? id
  if (kind === "image" && id)
    return images.get(id) ?? reported ?? t("jobs.target.image", { id: id.slice(0, 8) })
  return job.resource
}

/** The desktop a container runs: its image's desktop title, or the catalog image it names. */
export function desktopTitle(
  spec: ContainerSpec,
  title: string | undefined,
  images: Pick<ImageItem, "kind" | "reference" | "name">[]
) {
  return (
    title ||
    images.find((image) => image.kind === "catalog" && image.reference === spec.image)?.name ||
    (spec.image === LEGACY_IMAGE ? legacyTitle() : t("containers.customDesktop"))
  )
}

/** Where an image came from, as the image list names it. */
export function imageKind(kind: string) {
  const key = `images.kind.${kind}`
  return has(key) ? t(key) : kind
}

/** Whether an image can start a container now, and the color family for it. */
export function imageStatus(image: Pick<ImageItem, "kind" | "available" | "error">): {
  key: string
  tone: Tone
} {
  if (image.available === null || image.available === undefined)
    return { key: "unknown", tone: "muted" }
  if (image.available) return { key: "ready", tone: "success" }
  if (image.kind !== "catalog") return { key: "missing", tone: "danger" }
  // A catalog image that is not downloaded is fine: it downloads when it is used.
  return image.error ? { key: "unusable", tone: "danger" } : { key: "not_ready", tone: "muted" }
}

/**
 * What a container or a build names to start from an image: a catalog image by its reference, so
 * one that is not downloaded yet downloads first, and a saved image by its Docker ID.
 */
export const imageSource = (image: Pick<ImageItem, "kind" | "reference" | "image_id">) =>
  image.kind === "catalog" ? image.reference : (image.image_id ?? image.reference)

export function jobLogText(job: Pick<Job, "log" | "status" | "result">) {
  if (job.log?.trim()) return job.log
  if (job.status === "queued") return t("jobs.log.queued")
  if (job.status === "running") return t("jobs.log.running")
  if (job.status === "completed") {
    if (job.result?.deleted) {
      const name = job.result.name as string | undefined
      return [
        name ? t("jobs.log.deletedNamed", { name }) : t("jobs.log.deleted"),
        ...(job.result.volumes_preserved ? [t("jobs.log.volumesPreserved")] : []),
        ...(job.result.images_preserved ? [t("jobs.log.imagesPreserved")] : []),
      ].join(" ")
    }
    return t("jobs.log.completed")
  }
  return job.status === "failed" ? t("jobs.log.failed") : t("jobs.log.interrupted")
}

export type DownloadSummary = { text: string; value?: number; max?: number; percent?: number }

// Docker's per-layer counts, summarized by the manager while a job downloads an image.
export function downloadProgress(progress?: DownloadProgress | null): DownloadSummary | null {
  if (!progress) return null
  const { current = 0, total = null, layers = 0, layers_done: done = 0 } = progress
  if (!layers) return { text: t("progress.preparing") }
  const counted = plural(
    layers,
    { one: "progress.layers.one", other: "progress.layers.other" },
    { done: formatNumber(done) }
  )
  if (!total) {
    const so =
      total === 0 ? t("progress.checking") : t("progress.downloaded", { size: fileSize(current) })
    return { text: `${so} · ${counted}` }
  }
  if (current >= total && done < layers)
    return { value: 1, max: 1, percent: 100, text: `${t("progress.extracting")} · ${counted}` }
  return {
    value: current,
    max: total,
    percent: Math.floor((current * 100) / total),
    text: `${t("progress.partOf", { current: fileSize(current), total: fileSize(total) })} · ${counted}`,
  }
}

/** A running job's measured progress: layers for an image download, bytes for an archive. */
export function jobProgress(progress?: DownloadProgress | null): DownloadSummary | null {
  if (!progress?.transfer) return downloadProgress(progress)
  const { current = 0, total = null } = progress
  if (progress.transfer === "export")
    return {
      text: current
        ? t("progress.exported", { size: fileSize(current) })
        : t("progress.preparingImage"),
    }
  if (!total) return { text: t("progress.received", { size: fileSize(current) }) }
  return {
    value: current,
    max: total,
    percent: Math.floor((current * 100) / total),
    text: t("progress.receivedOf", { current: fileSize(current), total: fileSize(total) }),
  }
}

export type ImageChoice = { value: string; label: string; disabled: boolean }

type Choosable = Pick<
  ImageItem,
  "kind" | "reference" | "image_id" | "name" | "desktop" | "available" | "error"
>

export function imageChoices(
  images: Choosable[],
  defaultImage?: string,
  selected?: string
): ImageChoice[] {
  const choices = new Map<string, ImageChoice>()
  const catalog = images.filter((image) => image.kind === "catalog")
  // A catalog image downloads when it is used; one whose image does not match its labels cannot be used.
  for (const image of catalog)
    choices.set(image.reference, {
      value: image.reference,
      label:
        image.available || image.error
          ? image.name
          : `${image.name} · ${t("images.downloadsFirst")}`,
      disabled: !image.available && !!image.error,
    })
  if (defaultImage && !choices.has(defaultImage))
    choices.set(defaultImage, {
      value: defaultImage,
      label: defaultImage === LEGACY_IMAGE ? legacyTitle() : defaultImage,
      disabled: false,
    })
  for (const image of images) {
    if (image.kind === "catalog" || !image.image_id) continue
    choices.set(image.image_id, {
      value: image.image_id,
      label: image.desktop ? `${image.name} · ${image.desktop.title}` : image.name,
      disabled: image.available === false,
    })
  }
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

type TreeRecord = Pick<ImageItem, "id" | "image_id" | "parent_image_id">

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
      // A catalog image that is not downloaded has no Docker ID, so nothing builds on it.
      const children = (item.image_id && parents.get(item.image_id)) || []
      for (let i = children.length - 1; i >= 0; i--) stack.push([children[i], level + 1])
    }
  }
  for (const r of records) if (!r.parent_image_id || !ids.has(r.parent_image_id)) visit(r, 0)
  // Corrupt/cyclic metadata must not hide records or hang the UI.
  for (const r of records) if (!seen.has(r.id)) visit(r, 0)
  return result
}

/** What the settings dialog says before a change that recreates the container. */
export function restartNotice(fields: string[], running: boolean) {
  const names = fields.map((field) => {
    const key = `restart.field.${field}`
    return has(key) ? t(key) : field
  })
  const list = formatList(names)
  const subject = list.charAt(0).toLocaleUpperCase(getLocale()) + list.slice(1)
  return plural(
    names.length,
    { one: "restart.notice.one", other: "restart.notice.other" },
    {
      subject,
      effect: running ? t("restart.effect.running") : t("restart.effect.stopped"),
    }
  )
}

/** Writable places an upload can go: Downloads unless a read-only volume covers it, and writable volumes. */
export function uploadLocations(spec: Pick<ContainerSpec, "mounts">) {
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
    return t("uploads.subfolderError")
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

/** Sum of CPU and memory limits over running containers; an unlimited container adds nothing. */
export function limitTotals(items: ContainerItem[]) {
  let cpus = 0
  let memoryMb = 0
  let unlimited = 0
  for (const item of items) {
    if (item.state !== "running") continue
    const { spec } = item
    if (spec.cpus) cpus += spec.cpus
    if (spec.memory_mb) memoryMb += spec.memory_mb
    if (!spec.cpus || !spec.memory_mb) unlimited++
  }
  return { cpus, memoryMb, unlimited }
}
