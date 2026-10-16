// Paths and entries of a volume's files, as the volume browser shows them. A path is absolute
// within the volume, `/` being its root.

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
  if (!name) return "Enter a name."
  if (name === "." || name === "..") return "Choose another name."
  if (name.includes("/") || name.includes("\\")) return "Enter a name without slashes."
  if ([...name].some((c) => c.charCodeAt(0) < 32 || c.charCodeAt(0) === 127))
    return "Enter a name without control characters."
  if (new TextEncoder().encode(name).length > 255) return "Use at most 255 bytes."
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
