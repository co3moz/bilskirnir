import { RiFileTextLine } from "@remixicon/react"
import { cn } from "cn"

import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import { openDialog } from "@/lib/dialogs"
import { currentStage, jobName, jobProgress, jobTone } from "@/lib/format"
import { t } from "@/lib/i18n"
import { pendingState, type Pending } from "@/lib/pending"
import type { Job } from "@/lib/types"

const openJob = (id: string) => openDialog({ kind: "job", id })

/** Where a job making a container or an image is: its state, stage and measured progress. */
export function PendingStatus({ job }: { job: Job }) {
  const percent = job.status === "running" ? jobProgress(job.progress)?.percent : undefined
  const stage = currentStage(job)
  return (
    <div className="flex items-center gap-2">
      <StatusBadge state={pendingState(job)} tone={jobTone(job.status)} />
      {stage && <span className="max-w-48 truncate text-xs text-muted-foreground">{stage}</span>}
      {percent !== undefined && (
        <span className="text-xs text-muted-foreground tabular-nums">{percent}%</span>
      )}
    </div>
  )
}

/** The name of something still being made, opening its job's panel as the jobs page does. */
export function PendingName({ pending, className }: { pending: Pending; className?: string }) {
  return (
    <div className={cn("flex flex-col gap-0.5", className)}>
      <Button
        variant="link"
        className="h-auto w-fit p-0 text-left font-medium whitespace-normal text-foreground"
        onClick={() => openJob(pending.job.id)}
      >
        {pending.name}
      </Button>
      <span className="text-xs text-muted-foreground">{jobName(pending.job.kind)}</span>
    </div>
  )
}

/** The one thing a record that is still being made offers: its job's stages and log. */
export function JobLogButton({
  id,
  size = "sm",
  className,
}: {
  id: string
  size?: "sm" | "default"
  className?: string
}) {
  return (
    <Button variant="outline" size={size} className={className} onClick={() => openJob(id)}>
      <RiFileTextLine data-icon="inline-start" />
      {t("jobs.viewLog")}
    </Button>
  )
}
