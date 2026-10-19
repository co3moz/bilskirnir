import { toast } from "sonner"

import { api } from "./api"
import { archiveUrl } from "./archives"
import { formatBuildConfig } from "./build-config"
import { openDialog } from "./dialogs"
import { t } from "./i18n"
import { refreshAll } from "./queries"
import { navigate } from "./route"
import type { BuildConfig, ImageItem, JobStarted } from "./types"
import { fileSize } from "./uploads"
import { baseName, fileDownloadUrl, volumeArchiveUrl } from "./volume-files"

/** Queues a manager job, then shows its log so the outcome is visible. */
export async function runJob(path: string, body?: unknown, method?: string) {
  const result = await api<JobStarted>(path, body, method)
  toast.success(t("jobs.queued"))
  void refreshAll()
  openDialog({ kind: "job", id: result.job_id })
  return result
}

/** Queues several jobs without opening each log, as a bulk action does. */
export async function runJobs(requests: { path: string; body?: unknown }[]) {
  const results = await Promise.allSettled(
    requests.map(({ path, body }) => api<JobStarted>(path, body))
  )
  const failed = results.filter((result) => result.status === "rejected")
  const queued = results.length - failed.length
  if (queued) toast.success(queued === 1 ? "1 job queued." : `${queued} jobs queued.`)
  for (const message of new Set(failed.map((result) => (result.reason as Error).message)))
    toast.error(message)
  void refreshAll()
}

export const containerAction = (id: string, action: "start" | "stop" | "restart") =>
  runJob(`containers/${id}/action`, { action })

export const retrySetup = (id: string) => runJob(`containers/${id}/retry`, {})

export const setInternet = (id: string, enabled: boolean) =>
  runJob(`containers/${id}/internet`, { enabled })

export async function showLog(title: string, path: string) {
  const data = await api<{ log?: string; output?: string; status?: string }>(path)
  openDialog({ kind: "log", title, text: data.log ?? data.output ?? "", status: data.status })
}

/** Copies text. Browsers keep the clipboard API from plain HTTP pages, such as a LAN address, so
 * a hidden field and the older copy command stand in there. */
async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text)
    return true
  } catch {
    const field = document.createElement("textarea")
    field.value = text
    field.readOnly = true
    field.style.position = "fixed"
    field.style.opacity = "0"
    document.body.append(field)
    field.select()
    try {
      return document.execCommand("copy")
    } finally {
      field.remove()
    }
  }
}

/** Copies a built image's configuration as JSON, or shows it when the browser refuses to copy. */
export async function exportBuildConfig(image: Pick<ImageItem, "id" | "name">) {
  const text = formatBuildConfig(await api<BuildConfig>(`images/${image.id}/config`))
  if (await copyText(text)) toast.success(t("images.configCopied", { name: image.name }))
  else openDialog({ kind: "log", title: `${image.name} configuration`, text })
}

/** Runs an action from a button and reports a failure instead of throwing it away. */
export async function attempt(action: () => Promise<unknown>) {
  try {
    await action()
  } catch (error) {
    toast.error((error as Error).message)
  }
}

/** Lets the browser save what `href` answers, under the name the manager gives it. */
function save(href: string) {
  const link = document.createElement("a")
  link.href = href
  link.download = ""
  document.body.append(link)
  link.click()
  link.remove()
}

/** Points at the Jobs page, where an archive transfer's progress and any error appear. */
function archiving(message: string, description?: string) {
  toast.success(message, {
    description,
    action: { label: t("nav.jobs"), onClick: () => navigate({ page: "jobs" }) },
  })
  setTimeout(() => void refreshAll(), 1000)
}

/** Downloads a volume's files as a .tar.gz archive, recorded as a job like an image export. */
export function exportVolume(name: string) {
  save(volumeArchiveUrl(name))
  archiving(t("volumes.exporting", { name }))
}

/**
 * Downloads an image as a .tar.gz archive. The browser saves the stream itself; the manager
 * records the export as a job, where its progress and any Docker error appear.
 */
export function exportArchive(image: Pick<ImageItem, "id" | "name">) {
  save(archiveUrl(image.id))
  archiving(t("images.exporting", { name: image.name }), t("images.exportingDetail"))
}

/**
 * Downloads a file of a container or a volume (`base` is its API path) as it is. A file past
 * the transfer limit is refused here, since the manager would refuse it and the browser would
 * only show a failed download.
 */
export function downloadFile(base: string, path: string, size: number, limit: number) {
  if (limit && size > limit) {
    toast.error(t("downloads.tooLarge", { name: baseName(path), size: fileSize(limit) }))
    return
  }
  save(fileDownloadUrl(base, path))
}

/** Downloads a folder of a container or a volume as a .tar.gz, which the manager records as a job. */
export function downloadFolder(base: string, path: string) {
  save(fileDownloadUrl(base, path))
  archiving(t("downloads.archiving", { name: baseName(path) }), t("downloads.archivingDetail"))
}
