import { queryClient, quietWhileRestarting } from "./queries"

/**
 * Following an update or a settings change across the manager's restart. `thoric update` and
 * `thoric apply-settings` replace the manager container on their way: requests fail while it
 * is away, and the new one keeps no sessions and may serve the interface of another release. So
 * the page reloads as soon as a new manager answers.
 */

/** What one look at the manager found. */
export type Answer =
  | { kind: "away" }
  /** Sessions live in the manager's memory, so a new manager knows none. */
  | { kind: "signed-out" }
  | { kind: "status"; current: string; running: boolean }

/**
 * What follows a look, given the release the update started from and whether the manager was away
 * since: keep following, reload into the new manager, or stop, as the update ended in the manager
 * that started it.
 */
export function next(answer: Answer, from: string, away: boolean): "follow" | "reload" | "done" {
  if (answer.kind === "away") return "follow"
  if (answer.kind === "signed-out" || away || answer.current !== from) return "reload"
  return answer.running ? "follow" : "done"
}

const LOOK_EVERY = 1500
const UPDATED_FROM = "thoric.updatedFrom"
let following = false

/** What a restart is followed by: the update's status, or the configuration a settings change is on. */
type Restart = "update" | "settings"

async function look(restart: Restart, from: string): Promise<Answer> {
  try {
    const path = restart === "update" ? "/api/update" : "/api/configuration"
    const response = await fetch(path, { credentials: "same-origin", cache: "no-store" })
    if (response.status === 401) return { kind: "signed-out" }
    if (!response.ok) return { kind: "away" }
    const status = (await response.json()) as { current?: string; job_id?: string | null }
    // A settings change keeps the release, so only the manager being away tells it restarted.
    return { kind: "status", current: status.current ?? from, running: !!status.job_id }
  } catch {
    return { kind: "away" }
  }
}

/**
 * Follow a restart from release `from` until a new manager answers, then reload into it: an
 * update's, or a settings change's, which keeps the release.
 */
export function followRestart(restart: Restart, from: string, reloaded?: () => void) {
  if (following) return
  following = true
  quietWhileRestarting(true)
  let away = false
  const tick = async () => {
    const answer = await look(restart, from)
    const step = next(answer, from, away)
    if (step === "reload") {
      reloaded?.()
      location.reload()
      return
    }
    if (step === "done") {
      following = false
      quietWhileRestarting(false)
      // A new manager answers: anything it shows may have changed.
      void queryClient.invalidateQueries()
      return
    }
    away ||= answer.kind === "away"
    window.setTimeout(tick, LOOK_EVERY)
  }
  window.setTimeout(tick, LOOK_EVERY)
}

/** Follow an update from release `from` until a new manager answers, then reload into it. */
export function followUpdate(from: string) {
  followRestart("update", from, () => {
    try {
      sessionStorage.setItem(UPDATED_FROM, from)
    } catch {
      // The notice after the reload is a convenience.
    }
  })
}

/** The release an update reloaded the page from, read once. */
export function updatedFrom() {
  try {
    const from = sessionStorage.getItem(UPDATED_FROM)
    sessionStorage.removeItem(UPDATED_FROM)
    return from
  } catch {
    return null
  }
}
