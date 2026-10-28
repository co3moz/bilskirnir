// Container groups as the settings form and the container views show them. The manager decides
// which names are allowed and refuses one another member answers to; `nameLabel` and `groupNames`
// follow `groups.rs` there so the views can say what a container is reached as.
import type { ContainerItem, ContainerSpec } from "@/lib/types"

/** What the group and alias fields accept: lowercase DNS labels, separated by commas or spaces. */
export const NAMES_PATTERN = "[a-z0-9, -]*"

export function formatNames(names: readonly string[] = []) {
  return names.join(", ")
}

/** The names a field lists, in its order, each once. */
export function parseNames(text: string) {
  return [...new Set(text.split(/[\s,]+/).filter(Boolean))]
}

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

/** Every group a container lists, sorted, each once. */
export function knownGroups(items: readonly ContainerItem[]) {
  return [...new Set(items.flatMap((item) => item.spec.groups ?? []))].sort()
}

/** The other containers that share a group with `item`. */
export function groupPeers(item: ContainerItem, items: readonly ContainerItem[]) {
  const groups = item.spec.groups ?? []
  return items.filter(
    (other) => other.id !== item.id && (other.spec.groups ?? []).some((g) => groups.includes(g))
  )
}
