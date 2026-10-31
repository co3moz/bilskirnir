import * as React from "react"
import { RiAddLine, RiPencilLine, RiResetLeftLine, RiStopCircleLine } from "@remixicon/react"

import { ConceptIcon } from "@/components/concept"
import { CopyButton } from "@/components/copy-button"
import { JobStages, ProgressBar, ScriptSteps } from "@/components/job-stages"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import {
  DrawerClose,
  DrawerDescription,
  DrawerFooter,
  DrawerHeader,
  DrawerTitle,
} from "@/components/ui/drawer"
import { Skeleton } from "@/components/ui/skeleton"
import { attempt, cancelJob, editBuildJob, retryJob } from "@/lib/actions"
import { closeDialog, openDialog } from "@/lib/dialogs"
import { date, jobLogText, jobName, jobProgress, jobTone } from "@/lib/format"
import { t } from "@/lib/i18n"
import { useJob } from "@/lib/queries"
import { navigate } from "@/lib/route"
import { SideDrawer, type DialogProps } from "./side-drawer"

/** A job's live log, followed at the bottom unless the reader scrolled up. */
function JobLog({ text }: { text: string }) {
  const ref = React.useRef<HTMLPreElement>(null)
  const content = React.useRef<HTMLElement>(null)
  const follow = React.useRef(true)
  React.useLayoutEffect(() => {
    const log = ref.current
    if (log && follow.current) log.scrollTop = log.scrollHeight
  }, [text])
  React.useEffect(() => {
    const log = ref.current
    if (!log || !content.current) return
    // The log can grow after a render, once the mono font loads or the panel settles.
    const observer = new ResizeObserver(() => {
      if (follow.current) log.scrollTop = log.scrollHeight
    })
    observer.observe(content.current)
    observer.observe(log)
    return () => observer.disconnect()
  }, [])
  return (
    <pre
      ref={ref}
      onScroll={(event) => {
        const log = event.currentTarget
        follow.current = log.scrollHeight - log.scrollTop - log.clientHeight < 48
      }}
      className="min-h-48 flex-1 overflow-auto rounded-md border bg-muted/40 p-3 font-mono text-xs leading-relaxed break-words whitespace-pre-wrap"
    >
      <code ref={content} className="block">
        {text}
      </code>
    </pre>
  )
}

export function JobDrawer({ id, ...props }: DialogProps & { id: string }) {
  const job = useJob(id, props.open)
  const data = job.data
  const progress = data?.status === "running" ? jobProgress(data.progress) : null
  // A job with stages, such as a script build, shows its progress under the running stage.
  const staged = data?.stages?.length ? data : null
  const result = data?.result as
    { reference?: string; image_id?: string; container_id?: string } | null | undefined
  // A retry opens the new job's log, or the build form, in place of this one.
  const [busy, setBusy] = React.useState(false)
  const run = (action: () => Promise<unknown>) => async () => {
    setBusy(true)
    await attempt(action)
    setBusy(false)
  }

  return (
    <SideDrawer {...props} wide>
      <DrawerHeader className="gap-1">
        <DrawerTitle>{data ? jobName(data.kind) : t("jobs.log")}</DrawerTitle>
        <DrawerDescription asChild>
          <div className="flex flex-wrap items-center gap-2">
            {data ? (
              <>
                <StatusBadge state={data.status} tone={jobTone(data.status)} />
                <span>{date(data.created_at)}</span>
              </>
            ) : (
              <Skeleton className="h-5 w-40" />
            )}
          </div>
        </DrawerDescription>
      </DrawerHeader>
      {/* Scrolls when the stages and a script's steps leave the log less than its least height. */}
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-4 text-sm">
        {staged && <JobStages job={staged} progress={progress} />}
        {progress && !staged && <ProgressBar progress={progress} />}
        {/* A compose setup's steps, in the job that creates, resets or retries a container. */}
        {data && !staged && <ScriptSteps job={data} />}
        {job.error ? (
          <p className="text-destructive">{job.error.message}</p>
        ) : data ? (
          <JobLog text={jobLogText(data)} />
        ) : (
          <Skeleton className="h-48 w-full" />
        )}
        {result?.reference && (
          <p className="font-mono text-xs break-all text-muted-foreground">{result.reference}</p>
        )}
      </div>
      <DrawerFooter className="flex-row flex-wrap justify-end">
        {/* The log as it is at the click, also while the job is still writing it. */}
        {data?.log?.trim() && !job.error && (
          <CopyButton text={data.log} label={t("jobs.copyLog")} showLabel className="mr-auto" />
        )}
        {result?.image_id && result.reference && (
          <Button onClick={() => openDialog({ kind: "create", image: result.image_id })}>
            <RiAddLine data-icon="inline-start" />
            {t("job.create")}
          </Button>
        )}
        {result?.container_id && (
          <Button
            variant="outline"
            onClick={() => {
              closeDialog()
              navigate({ page: "containers" })
            }}
          >
            <ConceptIcon concept="container" data-icon="inline-start" />
            {t("jobs.viewContainers")}
          </Button>
        )}
        {data?.cancellable && (
          <Button variant="outline" disabled={busy} onClick={run(() => cancelJob(id))}>
            <RiStopCircleLine data-icon="inline-start" />
            {t("jobs.cancel")}
          </Button>
        )}
        {data?.retryable && data.kind === "build" && (
          <Button variant="outline" disabled={busy} onClick={run(() => editBuildJob(id))}>
            <RiPencilLine data-icon="inline-start" />
            {t("jobs.editRetry")}
          </Button>
        )}
        {data?.retryable && (
          <Button disabled={busy} onClick={run(() => retryJob(id))}>
            <RiResetLeftLine data-icon="inline-start" />
            {t("jobs.retry")}
          </Button>
        )}
        <DrawerClose asChild>
          <Button variant="outline">{t("common.done")}</Button>
        </DrawerClose>
      </DrawerFooter>
    </SideDrawer>
  )
}
