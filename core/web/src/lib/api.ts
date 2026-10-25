import { t } from "./i18n"

let unauthorized = () => {}

/** Called once a request learns that the session ended, so the app can show the sign-in page. */
export function onUnauthorized(handler: () => void) {
  unauthorized = handler
}
export const sessionExpired = () => unauthorized()

/** A request the manager answered with an error, and the HTTP status it answered with. */
export class ApiError extends Error {
  status: number

  constructor(message: string, status: number) {
    super(message)
    this.status = status
  }
}

export async function api<T = unknown>(
  path: string,
  body?: unknown,
  method = body === undefined ? "GET" : "POST"
): Promise<T> {
  const response = await fetch(`/api/${path}`, {
    method,
    credentials: "same-origin",
    // The manager refuses state changes without this header; it stands in for a CSRF token.
    headers: { "Content-Type": "application/json", "X-Bilskirnir-Request": "1" },
    ...(body !== undefined ? { body: JSON.stringify(body) } : {}),
  })
  if (response.status === 401 && path !== "login") {
    unauthorized()
    throw new Error(t("session.expired"))
  }
  if (!response.ok) {
    const data = await response.json().catch(() => ({}))
    throw new ApiError(data.error || t("api.failed", { status: response.status }), response.status)
  }
  return (response.status === 204 ? null : await response.json()) as T
}

export const wsUrl = (path: string) =>
  `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/api/${path}`
