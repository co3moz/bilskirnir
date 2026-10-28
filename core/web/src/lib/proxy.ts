import type { ContainerSpec, ProxyChoice, Settings } from "./types"

export type ProxyMode = "none" | "installation" | "custom"

/**
 * The proxy fields of the create and settings forms. `password` is a new one typed here, never the
 * kept one, which the manager does not send; `stored` says one is kept and `clear` removes it.
 */
export type ProxyDraft = {
  mode: ProxyMode
  url: string
  username: string
  bypass: string
  password: string
  stored: boolean
  clear: boolean
}

/**
 * A container's proxy as the form starts it. A new container follows the installation's proxy
 * where there is one, as a create request that leaves it out does.
 */
export function proxyDraft(spec: Partial<ContainerSpec>, settings?: Settings): ProxyDraft {
  const followed: ProxyChoice | null = settings?.proxy ? { mode: "installation" } : null
  const proxy = spec.proxy === undefined ? followed : spec.proxy
  const empty = { url: "", username: "", bypass: "", password: "", stored: false, clear: false }
  if (!proxy) return { mode: "none", ...empty }
  if (proxy.mode === "installation") return { mode: "installation", ...empty }
  return {
    ...empty,
    mode: "custom",
    url: proxy.url,
    username: proxy.username ?? "",
    bypass: proxy.bypass.join(", "),
    stored: !!proxy.password,
  }
}

/** The entries of a bypass list typed with commas or spaces between them. */
export function parseBypass(text: string): string[] {
  return text
    .split(/[\s,]+/)
    .map((entry) => entry.trim())
    .filter(Boolean)
}

/**
 * The request fields for a proxy draft. `proxy_password` is sent only to set a new password or to
 * remove the kept one; left out, the manager keeps the one there is for the same host.
 */
export function proxyBody(draft: ProxyDraft): {
  proxy: ProxyChoice | null
  proxy_password?: string | null
} {
  if (draft.mode === "none") return { proxy: null }
  if (draft.mode === "installation") return { proxy: { mode: "installation" } }
  const proxy: ProxyChoice = {
    mode: "custom",
    url: draft.url.trim(),
    username: draft.username.trim() || null,
    bypass: parseBypass(draft.bypass),
  }
  if (draft.password) return { proxy, proxy_password: draft.password }
  if (draft.clear) return { proxy, proxy_password: null }
  return { proxy }
}

/** What a proxy address looks like to the browser before the manager checks it in full. */
export const PROXY_URL_PATTERN = String.raw`(?:[Hh][Tt][Tt][Pp][Ss]?|[Ss][Oo][Cc][Kk][Ss]5):\/\/[^\s\/@?#]+:\d{1,5}\/?`
