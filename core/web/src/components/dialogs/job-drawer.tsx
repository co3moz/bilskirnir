import * as React from "react"
import { RiAddLine } from "@remixicon/react"

import { ConceptIcon } from "@/components/concept"
import { JobStages } from "@/components/job-stages"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import {
  DrawerClose,
  DrawerDescription,
  DrawerFooter,
  DrawerHeader,
  DrawerTitle,
} from "@/components/ui/drawer"
import { Progress } from "@/components/ui/progress"
import { Skeleton } from "@/components/ui/skeleton"
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
  const job = useJob(id)
  const data = job.data
  const progress = data?.status === "running" ? jobProgress(data.progress) : null
  // A job with stages, such as a script build, shows its progress under the running stage.
  const staged = data?.stages?.length ? data : null
  const result = data?.result as
    { reference?: string; image_id?: string; container_id?: string } | null | undefined

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
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden px-4 text-sm">
        {staged && <JobStages job={staged} progress={progress} />}
        {progress && !staged && (
          <div role="status" className="flex flex-col gap-2">
            {progress.max !== undefined && (
              <Progress
                value={(progress.value! * 100) / progress.max}
                aria-label={
                  data?.progress?.transfer ? t("jobs.archiveProgress") : t("jobs.downloadProgress")
                }
              />
            )}
            <p className="text-xs text-muted-foreground">{progress.text}</p>
          </div>
        )}
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
        <DrawerClose asChild>
          <Button variant="outline">{t("common.done")}</Button>
        </DrawerClose>
      </DrawerFooter>
    </SideDrawer>
  )
}
