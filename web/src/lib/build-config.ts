import type { BuildConfig } from "./types"

/** A pasted configuration: the script is required, the other fields keep the form's value. */
export type ImportedBuild = Partial<Omit<BuildConfig, "script">> & Pick<BuildConfig, "script">

/** The configuration as it is copied: the build request, keys in the form's order. */
export function formatBuildConfig({ name, base_image, internet, script }: BuildConfig) {
  return JSON.stringify({ name, base_image, internet, script }, null, 2)
}

/** Reads a pasted configuration, or throws the reason it cannot fill the build form. */
export function parseBuildConfig(text: string): ImportedBuild {
  let value: unknown
  try {
    value = JSON.parse(text)
  } catch {
    throw new Error("The configuration is not valid JSON.")
  }
  if (typeof value !== "object" || value === null || Array.isArray(value))
    throw new Error("The configuration must be a JSON object.")
  const { name, base_image, internet, script } = value as Record<string, unknown>
  if (typeof script !== "string" || !script.trim())
    throw new Error("The configuration has no setup script.")
  const config: ImportedBuild = { script }
  if (name !== undefined) {
    if (typeof name !== "string") throw new Error("The name must be text.")
    if ([...name].length > 100) throw new Error("The name must be at most 100 characters.")
    config.name = name
  }
  if (base_image !== undefined) {
    if (typeof base_image !== "string" || !base_image)
      throw new Error("The base image must be an image reference.")
    config.base_image = base_image
  }
  if (internet !== undefined) {
    if (typeof internet !== "boolean") throw new Error("Internet must be true or false.")
    config.internet = internet
  }
  return config
}
