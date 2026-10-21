import { quietWhileRestarting, refreshAll } from "./queries"
import type { UpdateStatus } from "./types"

/**
 * Following an update across the manager's restart. `bilskirnir update` replaces the manager
 * container on its way: requests fail while it is away, and the new one keeps no sessions and
 * serves the interface of its own release. So the page reloads as soon as a new manager answers.
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
const UPDATED_FROM = "bilskirnir.updatedFrom"
let following = false

async function look(): Promise<Answer> {
  try {
    const response = await fetch("/api/update", { credentials: "same-origin", cache: "no-store" })
    if (response.status === 401) return { kind: "signed-out" }
    if (!response.ok) return { kind: "away" }
    const status = (await response.json()) as UpdateStatus
    return { kind: "status", current: status.current, running: !!status.job_id }
  } catch {
    return { kind: "away" }
  }
}

/** Follow an update from release `from` until a new manager answers, then reload into it. */
export function followUpdate(from: string) {
  if (following) return
  following = true
  quietWhileRestarting(true)
  let away = false
  const tick = async () => {
    const answer = await look()
    const step = next(answer, from, away)
    if (step === "reload") {
      try {
        sessionStorage.setItem(UPDATED_FROM, from)
      } catch {
        // The notice after the reload is a convenience.
      }
      location.reload()
      return
    }
    if (step === "done") {
      following = false
      quietWhileRestarting(false)
      void refreshAll()
      return
    }
    away ||= answer.kind === "away"
    window.setTimeout(tick, LOOK_EVERY)
  }
  window.setTimeout(tick, LOOK_EVERY)
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
