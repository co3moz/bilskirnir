import { readCompose } from "./compose"
import { t } from "./i18n"
import { readIcon } from "./icons"
import { readServerDeclarations } from "./mcp"
import type { BuildConfig, BuildPart } from "./types"

/**
 * A pasted configuration or a template: a script or parts is required, the other fields keep
 * the form's value. `warnings` are what merging templates noticed; nothing sends them.
 */
export type ImportedBuild = Partial<BuildConfig> & { warnings?: string[] }

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

/** Reads a configuration already parsed from JSON, such as a template's. */
export function readBuildConfig(value: unknown): ImportedBuild {
  if (typeof value !== "object" || value === null || Array.isArray(value))
    throw new Error(t("build.config.object"))
  const fields = value as Record<string, unknown>
  const { name, description, base_image, internet, script, parts, icon, template_id } = fields
  const { template_ids, mcp } = fields
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
  if (mcp !== undefined && mcp !== null) {
    const servers = readServerDeclarations(mcp)
    if (servers.length) config.mcp = servers
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
