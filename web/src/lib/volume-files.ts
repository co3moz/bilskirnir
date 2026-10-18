// Paths and entries of a volume's files, as the volume browser shows them, and which volumes a
// container may mount, mounts or owns. A path is absolute within the volume, `/` being its root.
import { t } from "./i18n"
import type { Container, Volume } from "./types"

/** The volumes a container may mount: the manager refuses its own. */
export const mountableVolumes = (volumes: Volume[]) => volumes.filter((volume) => !volume.manager)

/** Where containers mount the volume `name`: through their volume settings, or their compose. */
export function volumeUsers(name: string, containers: Pick<Container, "id" | "spec" | "setup">[]) {
  return containers.flatMap((container) =>
    [
      ...container.spec.mounts.map((mount) => ({ ...mount, compose: false })),
      ...(container.setup?.mounts ?? []).map((mount) => ({ ...mount, compose: true })),
    ]
      .filter((mount) => mount.volume === name)
      .map((mount) => ({
        id: container.id,
        name: container.spec.name,
        target: mount.target,
        readonly: mount.readonly,
        compose: mount.compose,
      }))
  )
}

/** The volumes created for the container `id`, which deleting it can remove too. */
export const ownedVolumes = (volumes: Pick<Volume, "Name" | "owner">[], id: string) =>
  volumes
    .filter((volume) => volume.owner?.container_id === id)
    .map((volume) => volume.Name)
    .sort((a, b) => a.localeCompare(b))

/** One entry of `POST volumes/{name}/files/list`. */
export type VolumeEntry = {
  name: string
  type: "file" | "directory" | "symlink" | "special"
  size: number
  modified_at: number
  mode: number
}

export type VolumeListing = {
  path: string
  items: VolumeEntry[]
  total: number
  truncated: boolean
}

/** The largest file the browser opens as text; the manager reads at most this much at once. */
export const EDIT_LIMIT = 512 * 1024

export const joinPath = (directory: string, name: string) =>
  directory === "/" ? `/${name}` : `${directory}/${name}`

export function parentPath(path: string) {
  const cut = path.lastIndexOf("/")
  return cut <= 0 ? "/" : path.slice(0, cut)
}

/** The folders from the root to `path`, for a breadcrumb. */
export function pathCrumbs(path: string) {
  const crumbs = [{ name: "/", path: "/" }]
  let current = ""
  for (const part of path.split("/").filter(Boolean)) {
    current += `/${part}`
    crumbs.push({ name: part, path: current })
  }
  return crumbs
}

/** Why a new file or folder name cannot be used, or an empty string. */
export function entryNameError(name: string) {
  if (!name) return t("files.name.empty")
  if (name === "." || name === "..") return t("files.name.dots")
  if (name.includes("/") || name.includes("\\")) return t("files.name.slashes")
  if ([...name].some((c) => c.charCodeAt(0) < 32 || c.charCodeAt(0) === 127))
    return t("files.name.control")
  if (new TextEncoder().encode(name).length > 255) return t("files.name.long")
  return ""
}

/** Folders first, then files, each by name. */
export const sortEntries = (items: VolumeEntry[]) =>
  [...items].sort(
    (a, b) =>
      Number(b.type === "directory") - Number(a.type === "directory") ||
      a.name.localeCompare(b.name)
  )

export const volumeApi = (volume: string) => `volumes/${encodeURIComponent(volume)}`

export const volumeDownloadUrl = (volume: string, path: string) =>
  `/api/${volumeApi(volume)}/files/download?${new URLSearchParams({ path })}`

export const volumeArchiveUrl = (volume: string) => `/api/${volumeApi(volume)}/archive`

/** A volume name the manager accepts: letters, digits, dots, dashes and underscores. */
export const VOLUME_NAME = /^[a-zA-Z0-9][a-zA-Z0-9_.-]{1,127}$/

/** A volume name from an archive's file name: its extension dropped, other characters dashed. */
export function volumeArchiveName(fileName: string) {
  const name = fileName
    .replace(/\.(tar\.gz|tgz|tar|gz)$/i, "")
    .replace(/[^a-zA-Z0-9_.-]+/g, "-")
    .replace(/^[^a-zA-Z0-9]+/, "")
    .slice(0, 128)
  return VOLUME_NAME.test(name) ? name : ""
}
