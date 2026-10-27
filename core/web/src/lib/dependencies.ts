/**
 * The dependency list of the Dependencies page, which `core/manager/dependencies.py` writes while
 * the manager image builds and `GET /api/dependencies` returns as written.
 */

export type DependencyGroup = "ui" | "backend" | "infrastructure"
/** What a package or crate is used for: shipped, building only, or the tests and tools only. */
export type DependencyScope = "runtime" | "build" | "development"
export type DependencyKind = "image" | "package" | "tool" | "download" | "action" | "runner"
/** Where an infrastructure dependency is used: an image, the WSL package, Compose or the workflows. */
export type DependencyPlace = "manager" | "desktop" | "wsl" | "compose" | "ci"

export interface Dependency {
  group: DependencyGroup
  name: string
  /** Missing for a system package the image takes at whatever version its distribution has. */
  version?: string
  license?: string
  description?: string
  url?: string
  scope?: DependencyScope
  /**
   * Whether `core/web/package.json` or `core/manager/Cargo.toml` names it, rather than another
   * package.
   */
  direct?: boolean
  kind?: DependencyKind
  used_in?: DependencyPlace[]
  /** The image a system package is installed in. */
  source?: string
  /** The commit a workflow action is pinned to. */
  pin?: string
}

export interface DependencyList {
  format: number
  /** The release the list was made for. */
  version: string
  dependencies: Dependency[]
}

export const groups: DependencyGroup[] = ["ui", "backend", "infrastructure"]
const scopes: DependencyScope[] = ["runtime", "build", "development"]
const kinds: DependencyKind[] = ["image", "package", "tool", "download", "action", "runner"]

export const isScope = (value: string): value is DependencyScope =>
  (scopes as string[]).includes(value)

/** Everything, the directly named ones, a scope, a kind, or one license (`license:MIT`). */
export type DependencyFilter =
  "all" | "direct" | DependencyScope | DependencyKind | `license:${string}`

export const licenseFilter = (license: string): DependencyFilter => `license:${license}`

export function keeps(item: Dependency, filter: DependencyFilter) {
  if (filter === "all") return true
  if (filter === "direct") return !!item.direct
  if (filter.startsWith("license:")) return item.license === filter.slice("license:".length)
  return item.scope === filter || item.kind === filter
}

/**
 * The filters a group offers, each with how many of its dependencies it keeps, the ones that keep
 * none left out; and its licenses, the most used first, as one release asks of whoever ships it.
 */
export function groupFilters(items: Dependency[]) {
  const count = (filter: DependencyFilter) => items.filter((item) => keeps(item, filter)).length
  const kept = (filters: DependencyFilter[]) =>
    filters.map((value) => ({ value, count: count(value) })).filter((filter) => filter.count > 0)
  const used = new Map<string, number>()
  for (const item of items)
    if (item.license) used.set(item.license, (used.get(item.license) ?? 0) + 1)
  return {
    filters: [
      { value: "all" as DependencyFilter, count: items.length },
      ...kept(["direct", ...scopes, ...kinds]),
    ],
    licenses: [...used]
      .sort(([a, first], [b, second]) => second - first || a.localeCompare(b))
      .map(([license, count]) => ({ license, value: licenseFilter(license), count })),
  }
}

/** Whether every word of a search is in a dependency's name, version, license, description, image or places. */
export function matches(item: Dependency, search: string) {
  const words = search.toLowerCase().split(/\s+/).filter(Boolean)
  if (!words.length) return true
  const text = [
    item.name,
    item.version,
    item.license,
    item.description,
    item.source,
    item.pin,
    ...(item.used_in ?? []),
  ]
    .filter(Boolean)
    .join(" ")
    .toLowerCase()
  return words.every((word) => text.includes(word))
}

/** A commit as Git shows it in one line. */
export const shortCommit = (commit: string) => commit.slice(0, 7)

/** The name the list is saved under: `thoric-0.6.3-dependencies.json`. */
export const listFileName = (list: Pick<DependencyList, "version">) =>
  `thoric-${list.version.replace(/[^\w.-]/g, "_")}-dependencies.json`
