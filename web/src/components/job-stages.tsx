import * as React from "react"
import {
  RiCheckboxBlankCircleLine,
  RiCheckboxCircleFill,
  RiErrorWarningLine,
  RiIndeterminateCircleLine,
  RiLoader4Line,
} from "@remixicon/react"
import { cn } from "cn"

import { Progress } from "@/components/ui/progress"
import { duration, jobActive, jobStages, type DownloadSummary, type StageState } from "@/lib/format"
import { t } from "@/lib/i18n"
import type { Job } from "@/lib/types"

const icons: Record<StageState, React.ReactNode> = {
  done: <RiCheckboxCircleFill className="text-success" />,
  running: <RiLoader4Line className="animate-spin text-primary" />,
  failed: <RiErrorWarningLine className="text-destructive" />,
  pending: <RiCheckboxBlankCircleLine className="text-muted-foreground" />,
  skipped: <RiIndeterminateCircleLine className="text-muted-foreground" />,
}

const stateText = (state: StageState) =>
  ({
    done: t("state.completed"),
    running: t("state.running"),
    failed: t("state.failed"),
    pending: t("stages.pending"),
    skipped: t("stages.skipped"),
  })[state]

/** The current time in seconds, ticking while a job runs. */
function useNow(running: boolean) {
  const [now, setNow] = React.useState(() => Date.now() / 1000)
  React.useEffect(() => {
    if (!running) return
    const timer = setInterval(() => setNow(Date.now() / 1000), 1000)
    return () => clearInterval(timer)
  }, [running])
  return now
}

/**
 * A job's stages with the current one highlighted, the time each took and the whole run's. Only a
 * stage Docker measures, a base image download, gets a bar; the others show their time alone.
 */
export function JobStages({
  job,
  progress,
}: {
  job: Pick<Job, "status" | "stages" | "finished_at">
  progress: DownloadSummary | null
}) {
  const now = useNow(jobActive(job))
  const { rows, total } = jobStages(job, now)
  return (
    <section aria-label={t("stages.title")} className="flex flex-col gap-1">
      <ol className="flex flex-col gap-0.5">
        {rows.map((row) => {
          // The stage a job is in, or the one it stopped in.
          const current = row.state === "running" || row.state === "failed"
          return (
            <li
              key={row.name}
              aria-current={current ? "step" : undefined}
              className={cn(
                "flex flex-col gap-2 rounded-md px-2 py-1.5",
                current && "bg-muted text-foreground",
                !current && "text-muted-foreground"
              )}
            >
              <div className="flex items-center gap-2 [&_svg]:size-4 [&_svg]:shrink-0">
                <span aria-hidden>{icons[row.state]}</span>
                <span className={cn("min-w-0 flex-1 truncate", current && "font-medium")}>
                  {row.label}
                  <span className="sr-only">, {stateText(row.state)}</span>
                </span>
                <span className="text-xs whitespace-nowrap tabular-nums">
                  {row.seconds != null
                    ? duration(row.seconds)
                    : row.state === "skipped"
                      ? t("stages.skipped")
                      : null}
                </span>
              </div>
              {row.state === "running" && progress && (
                <div role="status" className="flex flex-col gap-1.5 pl-6">
                  {progress.max !== undefined && (
                    <Progress
                      value={(progress.value! * 100) / progress.max}
                      aria-label={t("jobs.downloadProgress")}
                      className="bg-background"
                    />
                  )}
                  <p className="text-xs text-muted-foreground">{progress.text}</p>
                </div>
              )}
              {row.state === "running" && !progress && row.name === "layer" && (
                <p className="pl-6 text-xs text-muted-foreground">{t("stages.layerHint")}</p>
              )}
            </li>
          )
        })}
      </ol>
      {total != null && (
        <p className="flex justify-between px-2 text-xs text-muted-foreground tabular-nums">
          <span>{t("stages.total")}</span>
          <span>{duration(total)}</span>
        </p>
      )}
    </section>
  )
}
