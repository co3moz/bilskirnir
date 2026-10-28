// Container groups as the settings form and the container views show them. The manager decides
// which names are allowed and refuses one another member answers to; `nameLabel` and `groupNames`
// follow `groups.rs` there so the views can say what a container is reached as.
import type { ContainerItem, ContainerSpec } from "@/lib/types"

/**
 * One group name or alias as `label` in groups.rs takes it: 1 to 63 lowercase letters, digits and
 * hyphens, without a hyphen at either end. Browsers read a `pattern` with the v flag, which wants
 * the hyphen in a class escaped.
 */
export const LABEL_PATTERN = "[a-z0-9]([a-z0-9\\-]{0,61}[a-z0-9])?"

// The Latin letters the manager writes without their marks; any other letter becomes a hyphen.
const FOLDED = Object.fromEntries(
  Object.entries({
    a: "àáâãäå",
    c: "ç",
    e: "èéêë",
    g: "ğ",
    i: "ıìíîï",
    n: "ñ",
    o: "òóôõöø",
    s: "şß",
    u: "ùúûü",
    y: "ýÿ",
  }).flatMap(([plain, marked]) => [...marked].map((c) => [c, plain]))
)

/** A container's name as a DNS label, or `null` when nothing of it is left. */
export function nameLabel(name: string) {
  let label = ""
  for (const c of name.toLowerCase()) {
    // What `İ` lowercases to, and the marks of letters written apart from them.
    if (/[\u0300-\u036f]/.test(c)) continue
    const plain = FOLDED[c] ?? c
    if (/^[a-z0-9]$/.test(plain)) label += plain
    else if (label && !label.endsWith("-")) label += "-"
  }
  return label.slice(0, 63).replace(/-+$/, "") || null
}

/** The names the members of a container's groups reach it by: hostname, name and aliases. */
export function groupNames(spec: ContainerSpec) {
  const names = [spec.hostname?.toLowerCase(), nameLabel(spec.name), ...(spec.aliases ?? [])]
  return [...new Set(names.filter((name): name is string => !!name))]
}

export type KnownGroup = { name: string; members: ContainerItem[] }

/** Every group a container lists, sorted, with the containers that list it. */
export function knownGroups(items: readonly ContainerItem[]): KnownGroup[] {
  const members = new Map<string, ContainerItem[]>()
  for (const item of items)
    for (const name of item.spec.groups ?? [])
      members.set(name, [...(members.get(name) ?? []), item])
  return [...members.keys()].sort().map((name) => ({ name, members: members.get(name)! }))
}

/** The other containers that share a group with `item`. */
export function groupPeers(item: ContainerItem, items: readonly ContainerItem[]) {
  const groups = item.spec.groups ?? []
  return items.filter(
    (other) => other.id !== item.id && (other.spec.groups ?? []).some((g) => groups.includes(g))
  )
}
