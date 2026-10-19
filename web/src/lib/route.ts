import { useSyncExternalStore } from "react"

export type Page = "dashboard" | "containers" | "images" | "templates" | "volumes" | "mcp" | "jobs"
export type Route = { page: Page } | { page: "desktop"; id: string }

const pages: Page[] = ["dashboard", "containers", "images", "templates", "volumes", "mcp", "jobs"]

/** Pages live in the URL fragment, so the manager serves one index.html for all of them. */
export function parseRoute(hash: string): Route {
  const [page, id] = hash.replace(/^#\/?/, "").split("/")
  if (page === "desktop" && id) return { page: "desktop", id: decodeURIComponent(id) }
  return { page: pages.includes(page as Page) ? (page as Page) : "dashboard" }
}

export const href = (route: Route) =>
  route.page === "desktop" ? `#/desktop/${encodeURIComponent(route.id)}` : `#/${route.page}`

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
