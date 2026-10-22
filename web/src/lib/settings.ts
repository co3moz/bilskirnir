import type { ConfigVariable, UpdateStatus } from "./types"

/** The sections of the settings page's configuration, in their order. */
export const SECTIONS = [
  "access",
  "mcp",
  "desktops",
  "gpu",
  "limits",
  "content",
  "manager",
  "misc",
] as const
export type Section = (typeof SECTIONS)[number]

/** The section of each setting the manager lists, by its name in `.env`. */
export const SECTION_OF: Record<string, Section> = {
  AUTH_MODE: "access",
  ADMIN_USERNAME: "access",
  ADMIN_PASSWORD: "access",
  SECURE_COOKIE: "access",
  ALLOWED_HOSTS: "access",
  ROOT_MCP_ENABLED: "mcp",
  MCP_AUTH_MODE: "mcp",
  MCP_TOKEN: "mcp",
  DESKTOP_IMAGE: "desktops",
  DESKTOP_USER_NAMESPACES: "desktops",
  AUDIO_ENABLED: "desktops",
  MICROPHONE_ENABLED: "desktops",
  WSL_GPU: "gpu",
  NVIDIA_COMPUTE: "gpu",
  GPU_ADAPTER: "gpu",
  UPLOAD_MAX_MB: "limits",
  IMAGE_ARCHIVE_MAX_MB: "limits",
  TEMPLATES_URL: "content",
  VOLUME_FILTER: "content",
  MANAGER_CONTAINER: "manager",
  UPDATE_CHECK: "manager",
}

/**
 * The settings by section, in the manager's order within each. A setting a newer manager added
 * and this page does not know yet is listed under `misc` rather than left out.
 */
export function sections(variables: ConfigVariable[]) {
  return SECTIONS.map((section) => ({
    section,
    variables: variables.filter((variable) => (SECTION_OF[variable.name] ?? "misc") === section),
  })).filter((entry) => entry.variables.length > 0)
}

/** How a setting's value reads: hidden, not set, or the text `.env` would hold. */
export function shownValue(
  variable: ConfigVariable
): { kind: "secret"; set: boolean } | { kind: "empty" } | { kind: "text"; text: string } {
  if (variable.secret) return { kind: "secret", set: variable.set }
  if (variable.value === null || variable.value === "") return { kind: "empty" }
  return { kind: "text", text: String(variable.value) }
}

/** Where this installation stands against the newest release. */
export function releaseState(
  update: Pick<UpdateStatus, "enabled" | "job_id" | "available" | "newest" | "error">
): "off" | "updating" | "available" | "current" | "failed" | "unchecked" {
  if (!update.enabled) return "off"
  if (update.job_id) return "updating"
  if (update.available) return "available"
  // A failed look keeps the release seen before, which still tells whether this one is current.
  if (update.newest) return "current"
  return update.error ? "failed" : "unchecked"
}
