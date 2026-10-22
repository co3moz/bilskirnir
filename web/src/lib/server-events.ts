import * as React from "react"
import { toast } from "sonner"

import { api, sessionExpired } from "./api"
import { openDialog, openedDialog } from "./dialogs"
import {
  endedJobs,
  jobNotice,
  keepStream,
  openEventSource,
  setStreamLive,
  staleKeys,
  withProgress,
  type ServerEvent,
} from "./events"
import { jobName, jobTarget } from "./format"
import { t } from "./i18n"
import { queryClient } from "./queries"
import type { ContainerItem, ImageItem, Job, Session } from "./types"

/** Lists to read again, gathered for a moment so a burst of events reads each once. */
let stale = new Set<string>()
let flush: ReturnType<typeof setTimeout> | undefined

function refresh(keys: string[]) {
  keys.forEach((key) => stale.add(key))
  flush ??= setTimeout(() => {
    const keys = stale
    stale = new Set()
    flush = undefined
    for (const key of keys)
      void queryClient.invalidateQueries(
        // The MCP page keeps several lists under one key.
        { queryKey: [key], exact: key !== "mcp" },
        { cancelRefetch: false }
      )
  }, 100)
}

/** Jobs already told about, so a job heard of twice, by its event and by a list, is told once. */
const told = new Set<string>()
/** Why each failed job failed, from its event; lists of jobs carry no error. */
const errors = new Map<string, string>()

/** Tell that a job ended, when that is worth a toast and its log is not open already. */
function announce(
  job: Pick<Job, "id" | "kind" | "resource" | "name" | "status" | "created_at" | "finished_at">
) {
  if (told.has(job.id)) return
  if (told.size > 500) told.clear()
  told.add(job.id)
  const notice = jobNotice(job)
  const open = openedDialog()
  if (!notice || (open?.kind === "job" && open.id === job.id)) return
  const containers = queryClient.getQueryData<ContainerItem[]>(["containers"]) ?? []
  const images = queryClient.getQueryData<ImageItem[]>(["images"]) ?? []
  const target =
    job.name ||
    jobTarget(
      job,
      new Map(containers.map((item) => [item.id, item.spec.name])),
      new Map(images.map((image) => [image.id, image.name]))
    )
  const action = jobName(job.kind)
  const error = errors.get(job.id)
  const options = {
    id: `job-${job.id}`,
    description: error ? [target, error].filter(Boolean).join(": ") : target || undefined,
    action: { label: t("jobs.viewLog"), onClick: () => openDialog({ kind: "job", id: job.id }) },
  }
  if (notice === "success") toast.success(t("events.jobCompleted", { action }), options)
  else if (job.status === "interrupted")
    toast.error(t("events.jobInterrupted", { action }), options)
  else toast.error(t("events.jobFailed", { action }), options)
}

function handle(event: ServerEvent) {
  if (event.type === "job") {
    const { job } = event
    refresh(staleKeys(job))
    void queryClient.invalidateQueries(
      { queryKey: ["jobs", job.id], exact: true },
      { cancelRefetch: false }
    )
    if (job.error) errors.set(job.id, job.error)
    if (errors.size > 100) errors.delete(errors.keys().next().value!)
    if (job.finished_at !== null) announce(job)
  } else if (event.type === "job-progress") {
    queryClient.setQueryData<Job[]>(["jobs"], (jobs) =>
      withProgress(jobs, event.id, event.progress)
    )
    queryClient.setQueryData<Job>(
      ["jobs", event.id],
      (job) => job && { ...job, progress: event.progress }
    )
  }
}

/** Whether the session still holds, before the stream is opened again after an error. */
async function sessionHolds() {
  const session = await api<Session>("session")
  if (session.auth_enabled && !session.authenticated) {
    sessionExpired()
    return false
  }
  return true
}

/**
 * Listen to the manager's event stream while signed in: lists are read again when a job changes,
 * and a toast tells when one ends. After a gap the page reads everything again, and the jobs that
 * ended meanwhile are told then, as are those the job list finds ended while the stream was down.
 */
export function useServerEvents() {
  React.useEffect(() => {
    let jobs: Job[] | undefined
    const unsubscribe = queryClient.getQueryCache().subscribe((event) => {
      if (event.type !== "updated" || event.action.type !== "success") return
      const key = event.query.queryKey
      if (key.length !== 1 || key[0] !== "jobs") return
      const read = event.query.state.data as Job[] | undefined
      if (jobs && read) endedJobs(jobs, read).forEach(announce)
      jobs = read
    })
    const stop = keepStream({
      open: openEventSource,
      onEvent: handle,
      // Events between the first read of the lists and the stream are not lost; after a gap,
      // nothing the page shows can be trusted.
      onReady: (resumed) => (resumed ? void queryClient.invalidateQueries() : refresh(["jobs"])),
      // Polling follows the stream: a list read now takes the pace that suits it.
      onLive: (live) => {
        setStreamLive(live)
        refresh(["jobs"])
      },
      check: sessionHolds,
    })
    return () => {
      stop()
      unsubscribe()
      setStreamLive(false)
    }
  }, [])
}
