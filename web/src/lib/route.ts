import { useSyncExternalStore } from "react"

export type Page =
  "dashboard" | "containers" | "images" | "templates" | "volumes" | "mcp" | "files" | "jobs"
export type Route =
  | { page: Exclude<Page, "files"> }
  /** The files page, with the container it shows when one was chosen. */
  | { page: "files"; container?: string }
  | { page: "desktop"; id: string }

const pages: Page[] = [
  "dashboard",
  "containers",
  "images",
  "templates",
  "volumes",
  "mcp",
  "files",
  "jobs",
]

/** Pages live in the URL fragment, so the manager serves one index.html for all of them. */
export function parseRoute(hash: string): Route {
  const [page, id] = hash.replace(/^#\/?/, "").split("/")
  if (page === "desktop" && id) return { page: "desktop", id: decodeURIComponent(id) }
  if (page === "files") return id ? { page, container: decodeURIComponent(id) } : { page }
  return { page: pages.includes(page as Page) ? (page as Exclude<Page, "files">) : "dashboard" }
}

export function href(route: Route) {
  if (route.page === "desktop") return `#/desktop/${encodeURIComponent(route.id)}`
  if (route.page === "files" && route.container)
    return `#/files/${encodeURIComponent(route.container)}`
  return `#/${route.page}`
}

export function navigate(route: Route) {
  location.hash = href(route)
}

const subscribe = (listener: () => void) => {
  window.addEventListener("hashchange", listener)
  return () => window.removeEventListener("hashchange", listener)
}

export function useRoute(): Route {
  const hash = useSyncExternalStore(subscribe, () => location.hash)
  return parseRoute(hash)
}
