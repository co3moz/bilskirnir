import { useSyncExternalStore } from "react"

export type Page =
  | "dashboard"
  | "containers"
  | "images"
  | "templates"
  | "volumes"
  | "mcp-endpoints"
  | "mcp-servers"
  | "mcp-inspector"
  | "files"
  | "jobs"
  | "activity"
  | "dependencies"
  | "settings"
export type Route =
  | { page: Exclude<Page, "files" | "activity"> }
  /** The files page, with the container it shows when one was chosen. */
  | { page: "files"; container?: string }
  /**
   * The agent activity log, narrowed to a container or a caller (`administrator` or
   * `endpoint:NAME`), with one call's details open when `entry` names it.
   */
  | { page: "activity"; container?: string; caller?: string; entry?: number }
  | { page: "desktop"; id: string }

const pages: Page[] = [
  "dashboard",
  "containers",
  "images",
  "templates",
  "volumes",
  "files",
  "jobs",
  "activity",
  "dependencies",
  "settings",
]

/** The MCP pages, each at `#/mcp/` and the part of its name after `mcp-`. */
const mcpPages = ["mcp-endpoints", "mcp-servers", "mcp-inspector"] as const
const isMcpPage = (page: string): page is (typeof mcpPages)[number] =>
  (mcpPages as readonly string[]).includes(page)

/** The activity page's filters, from the part of its link after `?`. */
function activityRoute(search: string): Route {
  const query = new URLSearchParams(search)
  const entry = Number(query.get("entry"))
  return {
    page: "activity",
    ...(query.get("container") ? { container: query.get("container")! } : {}),
    ...(query.get("caller") ? { caller: query.get("caller")! } : {}),
    ...(Number.isSafeInteger(entry) && entry > 0 ? { entry } : {}),
  }
}

/** Pages live in the URL fragment, so the manager serves one index.html for all of them. */
export function parseRoute(hash: string): Route {
  const link = hash.replace(/^#\/?/, "")
  const at = link.indexOf("?")
  const [path, search] = at < 0 ? [link, ""] : [link.slice(0, at), link.slice(at + 1)]
  const [page, id] = path.split("/")
  if (page === "desktop" && id) return { page: "desktop", id: decodeURIComponent(id) }
  if (page === "files") return id ? { page, container: decodeURIComponent(id) } : { page }
  if (page === "activity") return activityRoute(search)
  if (page === "mcp") {
    // `#/mcp` alone, the MCP page's link before it had pages of its own, opens the endpoints.
    const view = `mcp-${id}`
    return { page: isMcpPage(view) ? view : "mcp-endpoints" }
  }
  return {
    page: pages.includes(page as Page)
      ? (page as Exclude<Page, "files" | "activity">)
      : "dashboard",
  }
}

export function href(route: Route) {
  if (route.page === "desktop") return `#/desktop/${encodeURIComponent(route.id)}`
  if (route.page === "files" && route.container)
    return `#/files/${encodeURIComponent(route.container)}`
  if (route.page === "activity") {
    const query = new URLSearchParams()
    if (route.container) query.set("container", route.container)
    if (route.caller) query.set("caller", route.caller)
    if (route.entry) query.set("entry", String(route.entry))
    const search = query.toString()
    return search ? `#/activity?${search}` : "#/activity"
  }
  if (isMcpPage(route.page)) return `#/mcp/${route.page.slice("mcp-".length)}`
  return `#/${route.page}`
}

export function navigate(route: Route) {
  location.hash = href(route)
}

/** Go to `route` in place of the current entry of the browser's history, as a filter does. */
export function replaceRoute(route: Route) {
  location.replace(href(route))
}

const subscribe = (listener: () => void) => {
  window.addEventListener("hashchange", listener)
  return () => window.removeEventListener("hashchange", listener)
}

export function useRoute(): Route {
  const hash = useSyncExternalStore(subscribe, () => location.hash)
  return parseRoute(hash)
}
