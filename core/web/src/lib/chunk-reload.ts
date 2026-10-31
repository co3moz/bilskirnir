/**
 * A tab left open across a dashboard update still names the old build's chunks, which the new
 * manager no longer serves, so the first lazy page or chart it opens fails to load. Reloading
 * fetches the new build; a reload that fails the same way at once is not tried again.
 */

/** How long after a reload another failed chunk shows its error rather than reloading again. */
export const RELOAD_GUARD = 30_000

const KEY = "thoric-chunk-reload"

/** Whether `error` is a module or a stylesheet the browser could not load. */
export function isChunkError(error: unknown) {
  const message = error instanceof Error ? error.message : String(error ?? "")
  return /dynamically imported module|importing a module script failed|unable to preload css|failed to load module script/i.test(
    message
  )
}

type Store = Pick<Storage, "getItem" | "setItem">

/**
 * Whether the page may reload for a failed chunk now, which it records in `store`. Without a
 * store that works, such as in a private window that refuses one, it never reloads by itself,
 * since nothing would stop a loop.
 */
export function mayReload(store: Store | undefined, now = Date.now()) {
  try {
    if (!store) return false
    const last = Number(store.getItem(KEY))
    if (last && now - last >= 0 && now - last < RELOAD_GUARD) return false
    store.setItem(KEY, String(now))
    return true
  } catch {
    return false
  }
}

function session(): Store | undefined {
  try {
    return sessionStorage
  } catch {
    return undefined
  }
}

/** Reloads the page for a failed chunk once; returns whether it did. */
export function reloadForChunk(error: unknown) {
  if (!isChunkError(error) || !mayReload(session())) return false
  location.reload()
  return true
}
