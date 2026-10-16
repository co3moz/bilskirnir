import { RiPulseLine, RiCpuLine, RiRam2Line, RiComputerLine } from "@remixicon/react"

import { Badge } from "@/components/ui/badge"
import {
  Card,
  CardAction,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { jobActive, jobName, limitTotals, memory, needsAttention, stateLabel } from "@/lib/format"
import { formatNumber, plural, t } from "@/lib/i18n"
import { useContainers, useJobs, useSettings, useSystem } from "@/lib/queries"

function Metric({
  label,
  value,
  badge,
  line,
  hint,
}: {
  label: string
  value: React.ReactNode
  badge?: React.ReactNode
  line: React.ReactNode
  hint: React.ReactNode
}) {
  return (
    <Card className="@container/card">
      <CardHeader>
        <CardDescription>{label}</CardDescription>
        <CardTitle className="text-2xl font-semibold tabular-nums @[250px]/card:text-3xl">
          {value ?? <Skeleton className="h-8 w-16" />}
        </CardTitle>
        {badge && <CardAction>{badge}</CardAction>}
      </CardHeader>
      <CardFooter className="flex-col items-start gap-1.5 text-sm">
        <div className="line-clamp-1 flex gap-2 font-medium">{line}</div>
        <div className="line-clamp-1 text-muted-foreground">{hint}</div>
      </CardFooter>
    </Card>
  )
}

export function SectionCards() {
  const containers = useContainers().data
  const jobsQuery = useJobs()
  const jobs = jobsQuery.data
  const system = useSystem().data
  const settings = useSettings().data

  const running = containers?.filter((item) => item.state === "running").length ?? 0
  const attention = containers?.filter(needsAttention).length ?? 0
  const stopped = (containers?.length ?? 0) - running
  const active = jobs?.filter(jobActive).length ?? 0
  // Measured from when the list was fetched, so the count only changes with the list.
  const dayAgo = jobsQuery.dataUpdatedAt / 1000 - 86400
  const failedToday =
    jobs?.filter((job) => job.status !== "completed" && !jobActive(job) && job.created_at > dayAgo)
      .length ?? 0
  const latest = jobs?.[0]
  const limits = limitTotals(containers ?? [])
  const known = system?.connected

  return (
    <div className="grid grid-cols-1 gap-4 px-4 *:data-[slot=card]:bg-linear-to-t *:data-[slot=card]:from-primary/5 *:data-[slot=card]:to-card *:data-[slot=card]:shadow-xs lg:px-6 @xl/main:grid-cols-2 @5xl/main:grid-cols-4 dark:*:data-[slot=card]:bg-card">
      <Metric
        label={t("nav.containers")}
        value={containers?.length}
        badge={
          <Badge variant="outline">
            <RiComputerLine />
            {t("dashboard.running", { count: formatNumber(running) })}
          </Badge>
        }
        line={
          attention
            ? plural(attention, {
                one: "dashboard.attention.one",
                other: "dashboard.attention.other",
              })
            : t("dashboard.stopped", { count: formatNumber(stopped) })
        }
        hint={t("dashboard.containersHint")}
      />
      <Metric
        label={t("dashboard.jobsInProgress")}
        value={jobs ? active : undefined}
        badge={
          <Badge variant="outline" className={failedToday ? "text-destructive" : undefined}>
            <RiPulseLine />
            {failedToday
              ? t("dashboard.failedToday", { count: formatNumber(failedToday) })
              : t("dashboard.noFailures")}
          </Badge>
        }
        line={
          latest
            ? t("dashboard.lastJob", {
                job: jobName(latest.kind),
                state: stateLabel(latest.status),
              })
            : t("dashboard.noJobs")
        }
        hint={t("dashboard.jobsHint")}
      />
      <Metric
        label={t("dashboard.processors")}
        value={system ? (known ? system.cpus : t("common.unknown")) : undefined}
        badge={
          <Badge variant="outline">
            <RiCpuLine />
            {settings?.gpu_enabled ? t("dashboard.gpuReady") : t("dashboard.cpuOnly")}
          </Badge>
        }
        line={
          limits.cpus
            ? t("dashboard.cpuLimits", { count: formatNumber(limits.cpus) })
            : t("dashboard.noCpuLimits")
        }
        hint={
          known ? `Docker ${system?.version ?? ""}` : (system?.error ?? t("dashboard.dockerHost"))
        }
      />
      <Metric
        label={t("dashboard.memory")}
        value={system ? memory(system.memory_bytes) : undefined}
        badge={
          <Badge variant="outline">
            <RiRam2Line />
            {known
              ? t("dashboard.dockerContainers", { count: formatNumber(system?.containers ?? 0) })
              : t("dashboard.unavailable")}
          </Badge>
        }
        line={
          limits.memoryMb
            ? t("dashboard.memoryLimits", { size: memory(limits.memoryMb * 1024 ** 2) })
            : t("dashboard.noMemoryLimits")
        }
        hint={t("dashboard.memoryHint")}
      />
    </div>
  )
}
