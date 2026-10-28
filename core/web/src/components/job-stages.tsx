import * as React from "react"
import {
  RiArrowDownSLine,
  RiCheckboxBlankCircleLine,
  RiCheckboxCircleFill,
  RiErrorWarningLine,
  RiIndeterminateCircleLine,
  RiLoader4Line,
} from "@remixicon/react"
import { cn } from "cn"

import { Progress } from "@/components/ui/progress"
import {
  duration,
  jobActive,
  jobStages,
  scriptParts,
  type DownloadSummary,
  type ScriptStepRow,
  type StageState,
} from "@/lib/format"
import { t } from "@/lib/i18n"
import type { Job } from "@/lib/types"

const icons: Record<StageState, React.ReactNode> = {
  done: <RiCheckboxCircleFill className="text-success" />,
  running: <RiLoader4Line className="animate-spin text-primary" />,
  failed: <RiErrorWarningLine className="text-destructive" />,
  pending: <RiCheckboxBlankCircleLine className="text-muted-foreground" />,
  skipped: <RiIndeterminateCircleLine className="text-muted-foreground" />,
}

/** Stages in which Docker saves a layer and reports nothing until it is done. */
const layerStages = new Set(["layer", "commit"])

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

/** A row's time, or the word for a skipped one. */
function Took({ seconds, state }: { seconds: number | null; state: StageState }) {
  return (
    <span className="text-xs whitespace-nowrap tabular-nums">
      {seconds != null ? duration(seconds) : state === "skipped" ? t("stages.skipped") : null}
    </span>
  )
}

/** The progress bar of the running stage, or of a job without stages. */
export function ProgressBar({
  progress,
  className,
  barClassName,
}: {
  progress: DownloadSummary
  className?: string
  barClassName?: string
}) {
  return (
    <div role="status" className={cn("flex flex-col gap-1.5", className)}>
      {progress.max !== undefined && (
        <Progress
          value={(progress.value! * 100) / progress.max}
          aria-label={progress.title}
          className={barClassName}
        />
      )}
      <p className="text-xs break-words text-muted-foreground">{progress.text}</p>
    </div>
  )
}

function StepList({ steps, className }: { steps: ScriptStepRow[]; className?: string }) {
  return (
    <ol aria-label={t("scriptSteps.steps")} className={cn("flex flex-col gap-0.5", className)}>
      {steps.map((step) => (
        <li
          key={step.key}
          aria-current={step.state === "running" ? "step" : undefined}
          className={cn(
            "flex items-center gap-2 text-xs [&_svg]:size-3.5 [&_svg]:shrink-0",
            step.state === "running" || step.state === "failed"
              ? "text-foreground"
              : "text-muted-foreground"
          )}
        >
          <span aria-hidden>{icons[step.state]}</span>
          {step.number && <span className="tabular-nums">{step.number}</span>}
          <span className="min-w-0 flex-1 truncate" title={step.label}>
            {step.label}
            <span className="sr-only">, {stateText(step.state)}</span>
          </span>
          <Took seconds={step.seconds} state={step.state} />
        </li>
      ))}
    </ol>
  )
}

/**
 * The parts of a build's or a compose setup's scripts and the steps they named, with the time
 * each took. The part that runs, or the one the job stopped in, shows its steps; another opens on
 * a click. One part lists its steps alone, and nothing shows until a script names a step or there
 * are several parts.
 */
export function ScriptSteps({
  job,
  className,
}: {
  job: Pick<Job, "status" | "finished_at" | "script">
  className?: string
}) {
  const now = useNow(jobActive(job))
  const parts = scriptParts(job, now)
  const [toggled, setToggled] = React.useState<Record<string, boolean>>({})
  if (parts.length === 1) {
    if (!parts[0].steps.length) return null
    return (
      <div className={cn("max-h-60 overflow-y-auto", className)}>
        <StepList steps={parts[0].steps} />
      </div>
    )
  }
  if (!parts.length) return null
  return (
    <ol
      aria-label={t("scriptSteps.parts")}
      className={cn("flex max-h-72 flex-col gap-0.5 overflow-y-auto", className)}
    >
      {parts.map((part) => {
        const open =
          !!part.steps.length &&
          (toggled[part.key] ?? (part.state === "running" || part.state === "failed"))
        const current = part.state === "running" || part.state === "failed"
        const row = (
          <>
            <span aria-hidden>{icons[part.state]}</span>
            <span className={cn("min-w-0 flex-1 truncate text-left", current && "font-medium")}>
              {part.name}
              <span className="sr-only">, {stateText(part.state)}</span>
            </span>
            {part.cached ? (
              <span className="text-xs whitespace-nowrap">{t("scriptSteps.cached")}</span>
            ) : (
              <Took seconds={part.seconds} state={part.state} />
            )}
          </>
        )
        const rowClass = cn(
          "flex w-full items-center gap-2 rounded-md text-xs [&_svg]:size-3.5 [&_svg]:shrink-0",
          current ? "text-foreground" : "text-muted-foreground"
        )
        return (
          <li key={part.key} className="flex flex-col gap-1">
            {part.steps.length ? (
              <button
                type="button"
                aria-expanded={open}
                onClick={() => setToggled((all) => ({ ...all, [part.key]: !open }))}
                className={cn(
                  rowClass,
                  "outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
                )}
              >
                {row}
                <RiArrowDownSLine
                  aria-hidden
                  className={cn("text-muted-foreground transition-transform", open && "rotate-180")}
                />
              </button>
            ) : (
              <div className={rowClass}>{row}</div>
            )}
            {open && <StepList steps={part.steps} className="pl-5" />}
          </li>
        )
      })}
    </ol>
  )
}

/**
 * A job's stages with the current one highlighted, the time each took and the whole run's. A
 * stage with progress, a base image download or a running script, gets a bar; the others show
 * their time alone. The script stage lists the steps its scripts named.
 */
export function JobStages({
  job,
  progress,
}: {
  job: Pick<Job, "status" | "stages" | "finished_at" | "script">
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
                <Took seconds={row.seconds} state={row.state} />
              </div>
              {row.state === "running" && progress && (
                <ProgressBar progress={progress} className="pl-6" barClassName="bg-background" />
              )}
              {row.state === "running" && !progress && layerStages.has(row.name) && (
                <p className="pl-6 text-xs text-muted-foreground">{t("stages.layerHint")}</p>
              )}
              {row.name === "script" && row.state !== "pending" && (
                <ScriptSteps job={job} className="pl-6" />
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
