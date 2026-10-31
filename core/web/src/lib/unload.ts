/**
 * Work that ends with the page, such as uploads, the restore of a backup and a connected desktop
 * view: while one runs, the browser asks before a reload or a closed tab ends it.
 */

type Target = Pick<Window, "addEventListener" | "removeEventListener">

const warn = (event: BeforeUnloadEvent) => {
  event.preventDefault()
  // Browsers before the standard ask only when this is set.
  event.returnValue = ""
}

let running = 0

/** Holds the page while such work runs; returns what lets it go, which is safe to call twice. */
export function holdUnload(target: Target | undefined = globalThis.window) {
  if (!target) return () => {}
  if (running++ === 0) target.addEventListener("beforeunload", warn)
  let held = true
  return () => {
    if (!held) return
    held = false
    if (--running === 0) target.removeEventListener("beforeunload", warn)
  }
}
