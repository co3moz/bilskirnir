// The templates page: ready-made images listed on the project's wiki, or wherever TEMPLATES_URL
// points. A template is a build configuration as Export copies it; the list beside it names each
// one with a description and tags, and `<id>.png` next to it is its icon.
//
//   index.json   {"templates": [{"id": "unity", "name": "Unity Hub", "description": "…",
//                                "tags": ["games"], "requires": "0.5.0"}]}
//   unity.json   the build configuration
//   unity.png    the icon, at most 256 by 256 pixels
//
// Nothing checks a template before the list serves it, so every entry is checked here: one that
// cannot be offered shows why instead of breaking the page.
import { readBuildConfig, TEMPLATE_ID, type ImportedBuild } from "./build-config"
import { iconFromBlob } from "./icons"
import { t } from "./i18n"

export type Template = {
  id: string
  name: string
  description: string
  tags: string[]
  /** The oldest Bilskirnir release that can build it, such as `0.5.0`. */
  requires?: string
}

/** A list entry the page cannot offer, and why. */
export type BrokenTemplate = { id: string; reason: string }

export type TemplateList = { templates: Template[]; broken: BrokenTemplate[] }

const VERSION = /^\d+\.\d+\.\d+$/
const TAG_LIMIT = 8

/** Reads one list entry, or throws why it cannot be offered. */
function readTemplate(value: unknown): Template {
  if (typeof value !== "object" || value === null || Array.isArray(value))
    throw new Error(t("templates.broken.entry"))
  const { id, name, description = "", tags = [], requires } = value as Record<string, unknown>
  if (typeof id !== "string" || !TEMPLATE_ID.test(id)) throw new Error(t("templates.broken.id"))
  if (typeof name !== "string" || !name.trim() || [...name].length > 100)
    throw new Error(t("templates.broken.name"))
  if (typeof description !== "string" || [...description].length > 500)
    throw new Error(t("templates.broken.description"))
  if (
    !Array.isArray(tags) ||
    tags.length > TAG_LIMIT ||
    tags.some((tag) => typeof tag !== "string" || !tag.trim() || [...tag].length > 32)
  )
    throw new Error(t("templates.broken.tags", { count: TAG_LIMIT }))
  if (requires !== undefined && (typeof requires !== "string" || !VERSION.test(requires)))
    throw new Error(t("templates.broken.requires"))
  return {
    id,
    name: name.trim(),
    description: description.trim(),
    tags: (tags as string[]).map((tag) => tag.trim()),
    ...(requires ? { requires } : {}),
  }
}

/** Reads the list: the entries the page can offer, and the reasons for those it cannot. */
export function readTemplateList(value: unknown): TemplateList {
  const templates = (value as { templates?: unknown } | null)?.templates
  if (!Array.isArray(templates)) throw new Error(t("templates.listInvalid"))
  const list: TemplateList = { templates: [], broken: [] }
  const seen = new Set<string>()
  templates.forEach((entry, index) => {
    const id = String((entry as { id?: unknown } | null)?.id ?? `#${index + 1}`)
    try {
      const template = readTemplate(entry)
      if (seen.has(template.id)) throw new Error(t("templates.broken.duplicate"))
      seen.add(template.id)
      list.templates.push(template)
    } catch (error) {
      list.broken.push({ id, reason: (error as Error).message })
    }
  })
  return list
}

/** Compares two `major.minor.patch` versions; anything after the patch number is ignored. */
export function compareVersions(a: string, b: string) {
  const parts = (version: string) =>
    version
      .split(/[^\d.]/)[0]
      .split(".")
      .map((part) => Number(part) || 0)
  const [left, right] = [parts(a), parts(b)]
  for (let index = 0; index < 3; index++) {
    const difference = (left[index] ?? 0) - (right[index] ?? 0)
    if (difference) return Math.sign(difference)
  }
  return 0
}

/** Whether this manager can build the template. */
export const buildable = (template: Pick<Template, "requires">, version: string) =>
  !template.requires || compareVersions(version, template.requires) >= 0

/** A template's file beside the list: its configuration or its icon. */
export const templateFile = (list: string, id: string, extension: "json" | "png") =>
  new URL(`${encodeURIComponent(id)}.${extension}`, list).href

/** Whether a template matches a search, by its name, description or tags. */
export function matches(template: Template, search: string) {
  const words = search.toLocaleLowerCase().split(/\s+/).filter(Boolean)
  const text = [template.name, template.description, ...template.tags].join(" ").toLocaleLowerCase()
  return words.every((word) => text.includes(word))
}

/** Every tag of the list, each once, in alphabetical order. */
export const allTags = (templates: Template[]) =>
  [...new Set(templates.flatMap((template) => template.tags))].sort((a, b) => a.localeCompare(b))

// The list is public, so requests carry no cookies, and they ask the server whether a cached copy
// is still current, since a change on the wiki should show without waiting for the cache to expire.
async function fetchJson(url: string): Promise<unknown> {
  let response: Response
  try {
    response = await fetch(url, { credentials: "omit", cache: "no-cache" })
  } catch {
    throw new Error(t("templates.unreachable"))
  }
  if (!response.ok) throw new Error(t("templates.fetchFailed", { status: response.status }))
  try {
    return await response.json()
  } catch {
    throw new Error(t("templates.notJson"))
  }
}

export async function fetchTemplateList(list: string) {
  return readTemplateList(await fetchJson(list))
}

/**
 * A template as the build form takes it. The icon is the list's `<id>.png`, the picture its card
 * showed; one inside the configuration is used only when that file cannot be read.
 */
export async function loadTemplate(list: string, template: Template): Promise<ImportedBuild> {
  let config: ImportedBuild
  try {
    config = readBuildConfig(await fetchJson(templateFile(list, template.id, "json")))
  } catch (error) {
    throw new Error(
      t("templates.invalid", { name: template.name, reason: (error as Error).message }),
      { cause: error }
    )
  }
  const icon = await fetch(templateFile(list, template.id, "png"), { credentials: "omit" })
    .then((response) => (response.ok ? response.blob() : undefined))
    .then((blob) => blob && iconFromBlob(blob))
    .catch(() => undefined)
  return {
    ...config,
    name: config.name || template.name,
    description: config.description || template.description,
    ...(icon || config.icon ? { icon: icon ?? config.icon } : {}),
    template_id: template.id,
  }
}
