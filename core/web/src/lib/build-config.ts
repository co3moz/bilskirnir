import { readCompose } from "./compose"
import { t } from "./i18n"
import { readIcon } from "./icons"
import { readServerDeclarations } from "./mcp"
import type { BuildConfig, BuildPart, DraftPart, ServerDeclaration, TemplateOption } from "./types"

/**
 * A pasted configuration or a template: a script or parts is required, the other fields keep
 * the form's value. `options` are a template's choices and `warnings` what merging templates
 * noticed; the form writes the options into the scripts, and nothing sends either.
 */
export type ImportedBuild = Omit<Partial<BuildConfig>, "parts"> & {
  parts?: DraftPart[]
  options?: TemplateOption[]
  warnings?: string[]
}

/** What the manager accepts as a template id: lowercase letters, digits and dashes. */
export const TEMPLATE_ID = /^[a-z0-9][a-z0-9-]{0,63}$/

/**
 * The configuration as it is copied: the build request, keys in the form's order and the icon,
 * the longest line, last.
 */
export function formatBuildConfig({
  name,
  description,
  base_image,
  internet,
  script,
  parts,
  compose,
  inherit_compose,
  mcp,
  template_id,
  template_ids,
  icon,
}: BuildConfig) {
  return JSON.stringify(
    {
      name,
      ...(description ? { description } : {}),
      base_image,
      internet,
      ...(parts?.length ? { parts } : { script }),
      ...(compose ? { compose } : {}),
      ...(inherit_compose === false ? { inherit_compose } : {}),
      ...(mcp?.length ? { mcp } : {}),
      ...(template_id ? { template_id } : {}),
      ...(template_ids?.length ? { template_ids } : {}),
      ...(icon ? { icon } : {}),
    },
    null,
    2
  )
}

/** Reads a pasted configuration, or throws the reason it cannot fill the build form. */
export function parseBuildConfig(text: string): ImportedBuild {
  let value: unknown
  try {
    value = JSON.parse(text)
  } catch {
    throw new Error(t("build.config.json"))
  }
  return readBuildConfig(value)
}

function readPart(value: unknown): BuildPart {
  const { id, name, script } = (value ?? {}) as Record<string, unknown>
  if (
    typeof id !== "string" ||
    !TEMPLATE_ID.test(id) ||
    typeof name !== "string" ||
    !name.trim() ||
    [...name].length > 100 ||
    typeof script !== "string" ||
    !script.trim()
  )
    throw new Error(t("build.config.part"))
  return { id, name, script }
}

/** At most this many options, as the install form lists every one. */
const OPTION_LIMIT = 16
/** An option's name, the shell variable its script reads. */
const OPTION_NAME = /^[A-Z][A-Z0-9_]{0,31}$/

/** A template's options; each may name only desktop MCP servers the configuration declares. */
function readOptions(value: unknown, servers: ServerDeclaration[]): TemplateOption[] {
  if (!Array.isArray(value) || value.length > OPTION_LIMIT)
    throw new Error(t("build.config.options", { count: OPTION_LIMIT }))
  const declared = new Set(servers.map((server) => server.name))
  const seen = new Set<string>()
  return value.map((entry) => {
    const fields = (entry ?? {}) as Record<string, unknown>
    const { name, label, default: on } = fields
    const description = fields.description ?? undefined
    const mcp = fields.mcp ?? undefined
    if (typeof name !== "string" || !OPTION_NAME.test(name))
      throw new Error(t("build.config.optionName"))
    if (seen.has(name)) throw new Error(t("build.config.optionTwice", { name }))
    seen.add(name)
    if (
      typeof label !== "string" ||
      !label.trim() ||
      /[\r\n]/.test(label) ||
      [...label].length > 100
    )
      throw new Error(t("build.config.optionLabel", { name }))
    if (
      description !== undefined &&
      (typeof description !== "string" || [...description].length > 500)
    )
      throw new Error(t("build.config.optionDescription", { name }))
    if (typeof on !== "boolean") throw new Error(t("build.config.optionDefault", { name }))
    if (
      mcp !== undefined &&
      (!Array.isArray(mcp) ||
        mcp.some((server) => typeof server !== "string" || !declared.has(server)))
    )
      throw new Error(t("build.config.optionMcp", { name }))
    const about = (description as string | undefined)?.trim()
    const named = [...new Set((mcp ?? []) as string[])]
    return {
      name,
      label: label.trim(),
      ...(about ? { description: about } : {}),
      default: on,
      ...(named.length ? { mcp: named } : {}),
    }
  })
}

/** Reads a configuration already parsed from JSON, such as a template's. */
export function readBuildConfig(value: unknown): ImportedBuild {
  if (typeof value !== "object" || value === null || Array.isArray(value))
    throw new Error(t("build.config.object"))
  const fields = value as Record<string, unknown>
  const { name, description, base_image, internet, script, parts, icon, template_id } = fields
  const { template_ids, mcp, options, inherit_compose } = fields
  // Configurations exported before templates were called composes name it `template`.
  const compose = fields.compose ?? fields.template
  const config: ImportedBuild = {}
  if (parts !== undefined && parts !== null) {
    if (!Array.isArray(parts) || !parts.length) throw new Error(t("build.config.part"))
    if (script) throw new Error(t("build.config.scriptOrParts"))
    config.parts = parts.map(readPart)
  } else {
    if (typeof script !== "string" || !script.trim()) throw new Error(t("build.config.script"))
    config.script = script
  }
  if (name !== undefined) {
    if (typeof name !== "string") throw new Error(t("build.config.nameText"))
    if ([...name].length > 100) throw new Error(t("build.config.nameLong"))
    config.name = name
  }
  if (description !== undefined) {
    if (typeof description !== "string" || new TextEncoder().encode(description).length > 4096)
      throw new Error(t("build.config.description"))
    config.description = description
  }
  if (base_image !== undefined) {
    if (typeof base_image !== "string" || !base_image) throw new Error(t("build.config.base"))
    config.base_image = base_image
  }
  if (internet !== undefined) {
    if (typeof internet !== "boolean") throw new Error(t("build.config.internet"))
    config.internet = internet
  }
  if (compose !== undefined && compose !== null) config.compose = readCompose(compose)
  if (inherit_compose !== undefined && inherit_compose !== null) {
    if (typeof inherit_compose !== "boolean") throw new Error(t("build.config.inheritCompose"))
    config.inherit_compose = inherit_compose
  }
  if (mcp !== undefined && mcp !== null) {
    const servers = readServerDeclarations(mcp)
    if (servers.length) config.mcp = servers
  }
  // Options are a template's, written into its one script, so merged parts cannot have them.
  if (options !== undefined && options !== null) {
    if (config.parts) throw new Error(t("build.config.optionsParts"))
    const read = readOptions(options, config.mcp ?? [])
    if (read.length) config.options = read
  }
  if (icon !== undefined && icon !== null) config.icon = readIcon(icon)
  if (template_id !== undefined && template_id !== null) {
    if (typeof template_id !== "string" || !TEMPLATE_ID.test(template_id))
      throw new Error(t("build.config.template"))
    config.template_id = template_id
  }
  if (template_ids !== undefined && template_ids !== null) {
    if (
      !Array.isArray(template_ids) ||
      template_ids.some((id) => typeof id !== "string" || !TEMPLATE_ID.test(id))
    )
      throw new Error(t("build.config.template"))
    config.template_ids = template_ids as string[]
  }
  return config
}
