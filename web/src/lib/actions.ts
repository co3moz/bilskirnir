import { toast } from "sonner"

import { api } from "./api"
import { openDialog } from "./dialogs"
import { refreshAll } from "./queries"
import type { JobStarted } from "./types"

/** Queues a manager job, then shows its log so the outcome is visible. */
export async function runJob(path: string, body?: unknown, method?: string) {
  const result = await api<JobStarted>(path, body, method)
  toast.success("Job queued.")
  void refreshAll()
  openDialog({ kind: "job", id: result.job_id })
  return result
}

/** Queues several jobs without opening each log, as a bulk action does. */
export async function runJobs(requests: { path: string; body?: unknown }[]) {
  const results = await Promise.allSettled(
    requests.map(({ path, body }) => api<JobStarted>(path, body))
  )
  const failed = results.filter((result) => result.status === "rejected")
  const queued = results.length - failed.length
  if (queued) toast.success(queued === 1 ? "1 job queued." : `${queued} jobs queued.`)
  for (const message of new Set(failed.map((result) => (result.reason as Error).message)))
    toast.error(message)
  void refreshAll()
}

export const environmentAction = (id: string, action: "start" | "stop" | "restart") =>
  runJob(`environments/${id}/action`, { action })

export const retrySetup = (id: string) => runJob(`environments/${id}/retry`, {})

export const setInternet = (id: string, enabled: boolean) =>
  runJob(`environments/${id}/internet`, { enabled })

export async function showLog(title: string, path: string) {
  const data = await api<{ log?: string; output?: string; status?: string }>(path)
  openDialog({ kind: "log", title, text: data.log ?? data.output ?? "", status: data.status })
}

/** Runs an action from a button and reports a failure instead of throwing it away. */
export async function attempt(action: () => Promise<unknown>) {
  try {
    await action()
  } catch (error) {
    toast.error((error as Error).message)
  }
}
