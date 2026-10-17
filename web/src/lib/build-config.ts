import { readCompose } from "./compose"
import { t } from "./i18n"
import { readIcon } from "./icons"
import type { BuildConfig } from "./types"

/** A pasted configuration: the script is required, the other fields keep the form's value. */
export type ImportedBuild = Partial<Omit<BuildConfig, "script">> & Pick<BuildConfig, "script">

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
  compose,
  template_id,
  icon,
}: BuildConfig) {
  return JSON.stringify(
    {
      name,
      ...(description ? { description } : {}),
      base_image,
      internet,
      script,
      ...(compose ? { compose } : {}),
      ...(template_id ? { template_id } : {}),
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

/** Reads a configuration already parsed from JSON, such as a template's. */
export function readBuildConfig(value: unknown): ImportedBuild {
  if (typeof value !== "object" || value === null || Array.isArray(value))
    throw new Error(t("build.config.object"))
  const fields = value as Record<string, unknown>
  const { name, description, base_image, internet, script, icon, template_id } = fields
  // Configurations exported before templates were called composes name it `template`.
  const compose = fields.compose ?? fields.template
  if (typeof script !== "string" || !script.trim()) throw new Error(t("build.config.script"))
  const config: ImportedBuild = { script }
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
  if (icon !== undefined && icon !== null) config.icon = readIcon(icon)
  if (template_id !== undefined && template_id !== null) {
    if (typeof template_id !== "string" || !TEMPLATE_ID.test(template_id))
      throw new Error(t("build.config.template"))
    config.template_id = template_id
  }
  return config
}
