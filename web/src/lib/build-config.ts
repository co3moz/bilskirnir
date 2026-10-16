import { readCompose } from "./compose"
import { t } from "./i18n"
import type { BuildConfig } from "./types"

/** A pasted configuration: the script is required, the other fields keep the form's value. */
export type ImportedBuild = Partial<Omit<BuildConfig, "script">> & Pick<BuildConfig, "script">

/** The configuration as it is copied: the build request, keys in the form's order. */
export function formatBuildConfig({ name, base_image, internet, script, compose }: BuildConfig) {
  return JSON.stringify(
    { name, base_image, internet, script, ...(compose ? { compose } : {}) },
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
  if (typeof value !== "object" || value === null || Array.isArray(value))
    throw new Error(t("build.config.object"))
  const fields = value as Record<string, unknown>
  const { name, base_image, internet, script } = fields
  // Configurations exported before templates were called composes name it `template`.
  const compose = fields.compose ?? fields.template
  if (typeof script !== "string" || !script.trim()) throw new Error(t("build.config.script"))
  const config: ImportedBuild = { script }
  if (name !== undefined) {
    if (typeof name !== "string") throw new Error(t("build.config.nameText"))
    if ([...name].length > 100) throw new Error(t("build.config.nameLong"))
    config.name = name
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
  return config
}
