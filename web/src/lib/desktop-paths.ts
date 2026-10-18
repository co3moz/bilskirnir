// Folders of a desktop as the upload dialog and its folder browser see them. The manager makes
// the decision; these follow its rules so the dialog can say why before a file is sent.
import { t } from "./i18n"
import type { Container, Mount, Place } from "./types"

/** The desktop user's home: uploads may go anywhere in it. */
export const HOME = "/home/desktop"
export const DOWNLOADS = `${HOME}/Downloads`

const control = (text: string) =>
  [...text].some((c) => {
    const code = c.charCodeAt(0)
    return code < 32 || (code >= 127 && code < 160)
  })

/** An absolute folder without empty, `.` or `..` parts, backslashes or control characters. */
export const canonicalFolder = (path: string) =>
  path.startsWith("/") &&
  path.length <= 4096 &&
  !path.includes("\\") &&
  !control(path) &&
  (path === "/" ||
    path
      .slice(1)
      .split("/")
      .every((part) => part && part !== "." && part !== ".."))

/** The desktop's system folders, and the parents that hold more of them. */
const SYSTEM = [
  "/proc",
  "/sys",
  "/dev",
  "/run",
  "/tmp",
  "/usr",
  "/etc",
  "/bin",
  "/sbin",
  "/lib",
  "/lib64",
  "/var/run",
  "/var/lock",
  "/opt/VirtualGL",
]
const PARENTS = ["/", "/opt", "/var", "/home"]

const within = (path: string, root: string) => path === root || path.startsWith(`${root}/`)

/** A folder a folder parameter can name: canonical and outside the system folders. */
export const desktopFolder = (path: string) =>
  canonicalFolder(path) && !SYSTEM.some((folder) => within(path, folder)) && !PARENTS.includes(path)

type Folders = Pick<Container, "spec" | "setup">

/** Every volume the container mounts: those of its settings, then its compose's. */
export const containerMounts = (container: Folders): Mount[] => [
  ...container.spec.mounts,
  ...(container.setup?.mounts ?? []),
]

export const composePlaces = (container: Folders): Place[] => container.setup?.places ?? []

/**
 * Why files cannot be uploaded into `path`, or an empty string: anywhere in the home, a writable
 * volume or a folder the compose names will do, but nothing under a read-only volume.
 */
export function uploadProblem(container: Folders, path: string) {
  if (!canonicalFolder(path)) return t("folders.notCanonical")
  const mounts = containerMounts(container)
  if (mounts.some((mount) => mount.readonly && within(path, mount.target)))
    return t("folders.readOnly")
  if (
    within(path, HOME) ||
    mounts.some((mount) => within(path, mount.target)) ||
    composePlaces(container).some((place) => within(path, place.path))
  )
    return ""
  return t("folders.outside")
}

/** Whether files uploaded into `path` land in a volume, which snapshots leave out. */
export const inVolume = (container: Folders, path: string) =>
  containerMounts(container).some((mount) => within(path, mount.target))

export type QuickFolder = {
  path: string
  label: string
  kind: "home" | "compose" | "volume" | "recent"
}

/**
 * The folders the browser lists beside the current one: the home and its usual folders, those
 * the compose names, the writable volumes, then the ones used last that are not listed already.
 */
export function quickFolders(container: Folders, recent: string[] = []): QuickFolder[] {
  const folders: QuickFolder[] = [
    { path: HOME, label: t("folders.home"), kind: "home" },
    ...["Desktop", "Documents", "Downloads"].map((name) => ({
      path: `${HOME}/${name}`,
      label: name,
      kind: "home" as const,
    })),
    ...composePlaces(container).map((place) => ({
      path: place.path,
      label: place.label,
      kind: "compose" as const,
    })),
    ...containerMounts(container)
      .filter((mount) => !mount.readonly)
      .map((mount) => ({ path: mount.target, label: mount.volume, kind: "volume" as const })),
    ...recent.map((path) => ({ path, label: path, kind: "recent" as const })),
  ]
  const seen = new Set<string>()
  return folders.filter(
    (folder) =>
      !seen.has(folder.path) && seen.add(folder.path) && !uploadProblem(container, folder.path)
  )
}

/** Where a new upload goes: the folder used last, Downloads, or the home. */
export function defaultFolder(container: Folders, recent: string[] = []) {
  return [...recent, DOWNLOADS, HOME].find((path) => !uploadProblem(container, path)) ?? HOME
}

/** Whether a listing failed only because the folder is not there yet, which an upload creates. */
export const folderMissing = (message: string) => message.includes("[Errno 2]")

/** Why a folder cannot be listed, from the errno the manager's helper reports. */
export function listingProblem(message: string) {
  if (folderMissing(message)) return t("folders.missing")
  if (message.includes("[Errno 13]")) return t("folders.denied")
  if (message.includes("[Errno 20]")) return t("folders.notFolder")
  if (message.includes("[Errno 40]")) return t("folders.symlinkPath")
  return message
}

const RECENT_KEY = "bilskirnir.upload-folders"
const RECENT_LIMIT = 5

/** The folders this browser uploaded into last for a container, newest first. */
export function recentFolders(id: string): string[] {
  try {
    const stored = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "{}")?.[id]
    return Array.isArray(stored)
      ? stored.filter((path): path is string => typeof path === "string" && canonicalFolder(path))
      : []
  } catch {
    return []
  }
}

export function rememberFolder(id: string, path: string) {
  try {
    const stored = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "{}") ?? {}
    const recent = [path, ...recentFolders(id).filter((other) => other !== path)]
    stored[id] = recent.slice(0, RECENT_LIMIT)
    localStorage.setItem(RECENT_KEY, JSON.stringify(stored))
  } catch {
    // A browser without storage simply starts from Downloads again.
  }
}
