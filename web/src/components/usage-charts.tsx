import * as React from "react"
import { cn } from "cn"
import { Area, AreaChart, CartesianGrid, ReferenceLine, XAxis, YAxis } from "recharts"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { ChartContainer, ChartTooltip, type ChartConfig } from "@/components/ui/chart"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { memory as gigabytes } from "@/lib/format"
import { formatDate, formatNumber, t } from "@/lib/i18n"
import { useSystem } from "@/lib/queries"
import type { ContainerItem } from "@/lib/types"
import { fileSize } from "@/lib/uploads"
import {
  byFirstSample,
  metricValue,
  niceTicks,
  rangeStep,
  ranges,
  slotColor,
  stepRows,
  useUsage,
  type GpuDevice,
  type Metric,
  type RangeMinutes,
  type Row,
  type UsageSample,
  type UsageSeries,
} from "@/lib/usage"

const cores = (value: number) =>
  t("usage.unit.cpu", {
    value: formatNumber(value, { maximumFractionDigits: value < 10 ? 2 : 1 }),
  })
const rate = (bytes: number) => t("usage.unit.rate", { size: fileSize(Math.round(bytes)) })
const percent = (value: number) =>
  formatNumber(value / 100, { style: "percent", maximumFractionDigits: 0 })
const clock = (ms: number) =>
  formatDate(ms / 1000, { hour: "2-digit", minute: "2-digit", second: "2-digit" })
const tick = (ms: number) => formatDate(ms / 1000, { hour: "2-digit", minute: "2-digit" })
// Axis ticks are clean steps, so they need no more than one decimal.
const axisSize = (bytes: number) => {
  const [unit, name] = (
    [
      [1024 ** 3, "GiB"],
      [1024 ** 2, "MiB"],
      [1024, "KiB"],
    ] as const
  ).find(([size]) => bytes >= size) ?? [1, "B"]
  return `${formatNumber(bytes / unit, { maximumFractionDigits: 1 })} ${name}`
}

/** How each figure reads, on an axis and in a tooltip. */
const units: Record<
  Metric | "percent",
  { full: (value: number) => string; axis: (value: number) => string; bytes?: boolean }
> = {
  cpu: { full: cores, axis: (value) => formatNumber(value, { maximumFractionDigits: 2 }) },
  memory: { full: (value) => fileSize(Math.round(value)), axis: axisSize, bytes: true },
  disk: { full: rate, axis: axisSize, bytes: true },
  network: { full: rate, axis: axisSize, bytes: true },
  percent: { full: percent, axis: percent },
}

type Series = { key: string; label: string; color: string }

/** Values first and names second, every series at the hovered time, and their total. */
function UsageTooltip({
  active,
  payload,
  label,
  format,
  total,
}: {
  active?: boolean
  payload?: { dataKey?: unknown; name?: unknown; value?: unknown; color?: string }[]
  label?: unknown
  format: (value: number) => string
  total: boolean
}) {
  if (!active || !payload?.length) return null
  const items = payload.filter((item) => typeof item.value === "number")
  const sum = items.reduce((value, item) => value + (item.value as number), 0)
  return (
    <div className="grid min-w-40 gap-1.5 rounded-lg border border-border/50 bg-background px-2.5 py-1.5 text-xs shadow-xl">
      <div className="font-medium">{clock(Number(label))}</div>
      {[...items].reverse().map((item) => (
        <div key={String(item.dataKey)} className="flex items-center gap-2">
          <span
            aria-hidden
            className="h-0.5 w-3 shrink-0 rounded-full"
            style={{ backgroundColor: item.color }}
          />
          <span className="font-mono font-medium text-foreground tabular-nums">
            {format(item.value as number)}
          </span>
          <span className="min-w-0 truncate text-muted-foreground">{String(item.name)}</span>
        </div>
      ))}
      {total && items.length > 1 && (
        <div className="flex items-center gap-2 border-t pt-1.5">
          <span aria-hidden className="w-3 shrink-0" />
          <span className="font-mono font-medium text-foreground tabular-nums">{format(sum)}</span>
          <span className="text-muted-foreground">{t("usage.total")}</span>
        </div>
      )}
    </div>
  )
}

/**
 * One figure over time. Stacked, the top edge is the total of every container; otherwise each
 * series is drawn on its own, as a container's read and write are.
 */
function UsageChart({
  rows,
  series,
  unit,
  stacked,
  domainMax,
  limit,
  className = "h-[180px]",
}: {
  rows: Row[]
  series: Series[]
  unit: keyof typeof units
  stacked: boolean
  domainMax?: number
  limit?: number
  className?: string
}) {
  const config = Object.fromEntries(
    series.map((item) => [item.key, { label: item.label, color: item.color }])
  ) satisfies ChartConfig
  const format = units[unit]
  const first = rows[0]?.t ?? 0
  const last = rows[rows.length - 1]?.t ?? 0
  const highest = rows.reduce((top, row) => {
    const values = series.map((item) => row[item.key] ?? 0)
    return Math.max(
      top,
      stacked ? values.reduce((sum, value) => sum + value, 0) : Math.max(...values)
    )
  }, 0)
  // An idle chart still gets a scale: a tenth of a core, or a kibibyte.
  const floor = format.bytes ? 1024 : unit === "cpu" ? 0.1 : 1
  const ticks = niceTicks(domainMax ?? Math.max(highest, limit ?? 0, floor), format.bytes)
  return (
    <ChartContainer config={config} className={`aspect-auto w-full ${className}`}>
      <AreaChart data={rows} margin={{ left: 0, right: 8, top: 4 }}>
        <CartesianGrid vertical={false} />
        <XAxis
          dataKey="t"
          type="number"
          scale="time"
          domain={[first, last]}
          tickLine={false}
          axisLine={false}
          tickMargin={8}
          minTickGap={48}
          tickFormatter={tick}
        />
        <YAxis
          tickLine={false}
          axisLine={false}
          width={64}
          ticks={ticks}
          domain={[0, ticks[ticks.length - 1]]}
          allowDataOverflow
          tickFormatter={format.axis}
        />
        <ChartTooltip
          cursor={{ strokeWidth: 1 }}
          isAnimationActive={false}
          content={<UsageTooltip format={format.full} total={stacked} />}
        />
        {limit !== undefined && (
          <ReferenceLine
            y={limit}
            stroke="var(--muted-foreground)"
            strokeWidth={1}
            label={{
              value: t("usage.limit"),
              position: "insideTopRight",
              fill: "var(--muted-foreground)",
              fontSize: 11,
            }}
          />
        )}
        {series.map((item) => (
          <Area
            key={item.key}
            dataKey={item.key}
            name={item.label}
            type="monotoneX"
            stackId={stacked ? "usage" : undefined}
            // A stacked band is its color alone: a line on top would trace the total in the top
            // container's color, and a one-second spike is too narrow to keep its fill inside a border.
            stroke={stacked ? "none" : item.color}
            strokeWidth={stacked ? 0 : 2}
            fill={item.color}
            fillOpacity={stacked ? 0.75 : 0.1}
            isAnimationActive={false}
            dot={false}
            activeDot={{ r: 4, strokeWidth: 2, stroke: "var(--card)" }}
          />
        ))}
      </AreaChart>
    </ChartContainer>
  )
}

/** The range as toggles on a wide card, or as a menu on a narrow card and in a drawer. */
function RangePicker({
  value,
  onChange,
  menu = false,
}: {
  value: RangeMinutes
  onChange: (minutes: RangeMinutes) => void
  menu?: boolean
}) {
  const label = (minutes: number) => t(`usage.range.${minutes as RangeMinutes}`)
  const pick = (text: string) => text && onChange(Number(text) as RangeMinutes)
  return (
    <>
      <ToggleGroup
        type="single"
        value={String(value)}
        onValueChange={pick}
        variant="outline"
        className={cn(
          "hidden *:data-[slot=toggle-group-item]:px-3!",
          !menu && "@[640px]/card:flex"
        )}
        aria-label={t("usage.timeRange")}
      >
        {ranges.map((range) => (
          <ToggleGroupItem key={range.minutes} value={String(range.minutes)}>
            {label(range.minutes)}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
      <Select value={String(value)} onValueChange={pick}>
        <SelectTrigger
          className={cn("flex w-40", !menu && "@[640px]/card:hidden")}
          size="sm"
          aria-label={t("usage.timeRange")}
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent className="rounded-xl">
          {ranges.map((range) => (
            <SelectItem key={range.minutes} value={String(range.minutes)} className="rounded-lg">
              {label(range.minutes)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </>
  )
}

/** A figure's name and its value at the end of the range, above its chart. */
function Panel({
  title,
  value,
  hint,
  children,
}: {
  title: string
  value?: string
  hint?: string
  children: React.ReactNode
}) {
  return (
    <section className="flex min-w-0 flex-col gap-2 rounded-xl border p-3">
      <header className="flex flex-wrap items-baseline justify-between gap-x-3">
        <h3 className="text-sm font-medium">{title}</h3>
        {value && (
          <p className="text-sm">
            <span className="font-semibold">{value}</span>
            {hint && <span className="text-muted-foreground"> {hint}</span>}
          </p>
        )}
      </header>
      {children}
    </section>
  )
}

const lastRowSum = (rows: Row[], series: Series[]) =>
  series.reduce((sum, item) => sum + (rows[rows.length - 1]?.[item.key] ?? 0), 0)

const seriesKey = (id: string) => `c${id}`

/** The latest sample if it is recent; a stopped container has none. */
function current(series: UsageSeries, latest: number) {
  const sample = series.samples[series.samples.length - 1]
  return sample && latest - sample.t <= 5000 ? sample : undefined
}

function CurrentTable({
  containers,
  latest,
}: {
  containers: { series: UsageSeries; color: string }[]
  latest: number
}) {
  const dash = <span className="text-muted-foreground">{t("usage.stopped")}</span>
  return (
    <div className="overflow-x-auto rounded-xl border">
      <Table>
        <TableHeader className="bg-muted">
          <TableRow>
            <TableHead>{t("containers.container")}</TableHead>
            <TableHead className="text-right">{t("usage.cpu")}</TableHead>
            <TableHead className="text-right">{t("usage.memory")}</TableHead>
            <TableHead className="text-right">{t("usage.diskRead")}</TableHead>
            <TableHead className="text-right">{t("usage.diskWrite")}</TableHead>
            <TableHead className="text-right">{t("usage.netIn")}</TableHead>
            <TableHead className="text-right">{t("usage.netOut")}</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {containers.map(({ series, color }) => {
            const sample = current(series, latest)
            return (
              <TableRow key={series.id}>
                <TableCell className="max-w-48">
                  <span className="flex items-center gap-2">
                    <span
                      aria-hidden
                      className="size-2.5 shrink-0 rounded-[2px]"
                      style={{ backgroundColor: color }}
                    />
                    <span className="truncate">{series.name}</span>
                  </span>
                </TableCell>
                {sample ? (
                  <>
                    <TableCell className="text-right tabular-nums">{cores(sample.cpu)}</TableCell>
                    <TableCell className="text-right tabular-nums">
                      {fileSize(sample.memory)}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {rate(sample.disk_read)}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {rate(sample.disk_write)}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">{rate(sample.net_rx)}</TableCell>
                    <TableCell className="text-right tabular-nums">{rate(sample.net_tx)}</TableCell>
                  </>
                ) : (
                  <TableCell colSpan={6} className="text-right">
                    {dash}
                  </TableCell>
                )}
              </TableRow>
            )
          })}
        </TableBody>
      </Table>
    </div>
  )
}

function GpuPanels({
  devices,
  state,
  reason,
  end,
  minutes,
}: {
  devices: GpuDevice[]
  state: string
  reason: string | null
  end: number
  minutes: RangeMinutes
}) {
  if (state === "disabled") return null
  if (!devices.length) {
    const text =
      state === "unavailable"
        ? t("usage.gpu.unavailable", { reason: reason ?? t("common.unknown") })
        : state === "sampling"
          ? t("usage.waiting")
          : t("usage.gpu.idle")
    return (
      <Panel title={t("usage.gpu")}>
        <p className="text-sm text-muted-foreground">{text}</p>
        <p className="text-xs text-muted-foreground">{t("usage.gpuHint")}</p>
      </Panel>
    )
  }
  return devices.map((device) => {
    const series = [{ key: "gpu", label: device.name, color: "var(--primary)" }]
    const rows = stepRows(
      [{ key: "gpu", samples: device.samples, value: (sample) => sample.utilization }],
      end,
      minutes,
      rangeStep(minutes)
    )
    const last = device.samples[device.samples.length - 1]
    const used =
      last?.memory != null && device.memory_total
        ? t("usage.gpuMemory", {
            used: fileSize(last.memory),
            total: fileSize(device.memory_total),
          })
        : undefined
    return (
      <Panel
        key={device.index}
        title={`${t("usage.gpu")} · ${device.name}`}
        value={last?.utilization != null ? percent(last.utilization) : undefined}
        hint={used}
      >
        <UsageChart rows={rows} series={series} unit="percent" stacked={false} domainMax={100} />
        <p className="text-xs text-muted-foreground">{t("usage.gpuHint")}</p>
      </Panel>
    )
  })
}

function UsageStatus({ children }: { children: React.ReactNode }) {
  return <p className="py-8 text-center text-sm text-muted-foreground">{children}</p>
}

const panels: { metric: Metric; title: () => string }[] = [
  { metric: "cpu", title: () => t("usage.cpu") },
  { metric: "memory", title: () => t("usage.memory") },
  { metric: "disk", title: () => t("usage.disk") },
  { metric: "network", title: () => t("usage.network") },
]

/** Every container's use of the host, stacked so the top edge is the total. */
export function UsageOverview() {
  const { history, error, slots } = useUsage()
  const system = useSystem().data
  const [minutes, setMinutes] = React.useState<RangeMinutes>(5)
  const containers = React.useMemo(
    () =>
      byFirstSample(history?.containers ?? [])
        .filter((series) => series.samples.length)
        .map((series) => ({ series, slot: slots.get(series.id) }))
        .sort((a, b) => (a.slot ?? Infinity) - (b.slot ?? Infinity))
        .map(({ series, slot }) => ({ series, color: slotColor(slot), slot })),
    [history, slots]
  )
  const end = history?.latest ?? 0
  const step = rangeStep(minutes)
  // Containers past the last color are drawn together as one series.
  const named = containers.filter((item) => item.slot !== undefined)
  const others = containers.filter((item) => item.slot === undefined)
  const series: Series[] = [
    ...named.map((item) => ({
      key: seriesKey(item.series.id),
      label: item.series.name,
      color: item.color,
    })),
    ...(others.length
      ? [{ key: "other", label: t("usage.others"), color: slotColor(undefined) }]
      : []),
  ]
  const charts = panels.map(({ metric, title }) => {
    const value = metricValue[metric]
    const rows = stepRows(
      [
        ...named.map((item) => ({
          key: seriesKey(item.series.id),
          samples: item.series.samples,
          value,
        })),
        ...others.map((item) => ({ key: "other", samples: item.series.samples, value })),
      ],
      end,
      minutes,
      step
    )
    return { metric, title: title(), rows }
  })
  const hosts: Partial<Record<Metric, string>> = {
    cpu: system?.cpus ? t("usage.ofCpus", { count: formatNumber(system.cpus) }) : undefined,
    memory: system?.memory_bytes
      ? t("usage.of", { total: gigabytes(system.memory_bytes) })
      : undefined,
  }

  return (
    <Card className="@container/card">
      <CardHeader>
        <CardTitle>{t("usage.title")}</CardTitle>
        <CardDescription>{t("usage.description")}</CardDescription>
        <CardAction>
          <RangePicker value={minutes} onChange={setMinutes} />
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {(history && !history.available) || (error && !history) ? (
          <Alert variant="destructive">
            <AlertTitle>{t("usage.unavailable")}</AlertTitle>
            <AlertDescription>{history?.error ?? error}</AlertDescription>
          </Alert>
        ) : null}
        {!history ? (
          <Skeleton className="h-[220px] rounded-xl" />
        ) : !containers.length ? (
          <UsageStatus>{t("usage.empty")}</UsageStatus>
        ) : (
          <>
            <ul className="flex flex-wrap gap-x-4 gap-y-1 text-xs" aria-label={t("usage.legend")}>
              {series.map((item) => (
                <li key={item.key} className="flex min-w-0 items-center gap-1.5">
                  <span
                    aria-hidden
                    className="size-2.5 shrink-0 rounded-[2px]"
                    style={{ backgroundColor: item.color }}
                  />
                  <span className="truncate">{item.label}</span>
                </li>
              ))}
            </ul>
            <div className="grid gap-4 @3xl/card:grid-cols-2">
              {charts.map((chart) => (
                <Panel
                  key={chart.metric}
                  title={chart.title}
                  value={units[chart.metric].full(lastRowSum(chart.rows, series))}
                  hint={hosts[chart.metric]}
                >
                  <UsageChart rows={chart.rows} series={series} unit={chart.metric} stacked />
                </Panel>
              ))}
              <GpuPanels
                devices={history.gpu.devices}
                state={history.gpu.state}
                reason={history.gpu.reason}
                end={end}
                minutes={minutes}
              />
            </div>
            <CurrentTable containers={containers} latest={end} />
          </>
        )}
      </CardContent>
    </Card>
  )
}

/** One container's use, with its limits, for its details drawer. */
export function ContainerUsage({ item }: { item: ContainerItem }) {
  const { history, error } = useUsage()
  const [minutes, setMinutes] = React.useState<RangeMinutes>(5)
  const series = history?.containers.find((entry) => entry.id === item.id)
  const end = history?.latest ?? 0
  const step = rangeStep(minutes)
  const samples = series?.samples ?? []
  const rows = (keys: [string, (sample: UsageSample) => number][]) =>
    stepRows(
      keys.map(([key, value]) => ({ key, samples, value })),
      end,
      minutes,
      step
    )
  const sample = series && current(series, end)
  const charts = [
    {
      title: t("usage.cpu"),
      unit: "cpu" as const,
      series: [{ key: "cpu", label: t("usage.cpu"), color: slotColor(0) }],
      rows: rows([["cpu", metricValue.cpu]]),
      value: sample && cores(sample.cpu),
      limit: item.spec.cpus ?? undefined,
    },
    {
      title: t("usage.memory"),
      unit: "memory" as const,
      series: [{ key: "memory", label: t("usage.memory"), color: slotColor(0) }],
      rows: rows([["memory", metricValue.memory]]),
      value: sample && fileSize(sample.memory),
      limit: item.spec.memory_mb ? item.spec.memory_mb * 1024 ** 2 : undefined,
    },
    {
      title: t("usage.disk"),
      unit: "disk" as const,
      series: [
        { key: "read", label: t("usage.diskRead"), color: slotColor(0) },
        { key: "write", label: t("usage.diskWrite"), color: slotColor(1) },
      ],
      rows: rows([
        ["read", (s) => s.disk_read],
        ["write", (s) => s.disk_write],
      ]),
      value: sample && `${rate(sample.disk_read)} / ${rate(sample.disk_write)}`,
    },
    {
      title: t("usage.network"),
      unit: "network" as const,
      series: [
        { key: "in", label: t("usage.netIn"), color: slotColor(0) },
        { key: "out", label: t("usage.netOut"), color: slotColor(1) },
      ],
      rows: rows([
        ["in", (s) => s.net_rx],
        ["out", (s) => s.net_tx],
      ]),
      value: sample && `${rate(sample.net_rx)} / ${rate(sample.net_tx)}`,
    },
  ]
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="text-xs text-muted-foreground">{t("usage.title")}</h3>
        <RangePicker value={minutes} onChange={setMinutes} menu />
      </div>
      {history && !history.available && <p className="text-xs text-destructive">{history.error}</p>}
      {!history ? (
        error ? (
          <p className="text-xs text-destructive">{error}</p>
        ) : (
          <Skeleton className="h-[140px] rounded-xl" />
        )
      ) : !samples.length ? (
        <p className="text-xs text-muted-foreground">{t("usage.noSamples")}</p>
      ) : (
        charts.map((chart) => (
          <Panel key={chart.unit} title={chart.title} value={chart.value ?? t("usage.stopped")}>
            {chart.series.length > 1 && (
              <ul className="flex gap-4 text-xs" aria-label={t("usage.legend")}>
                {chart.series.map((entry) => (
                  <li key={entry.key} className="flex items-center gap-1.5">
                    <span
                      aria-hidden
                      className="h-0.5 w-3 shrink-0 rounded-full"
                      style={{ backgroundColor: entry.color }}
                    />
                    {entry.label}
                  </li>
                ))}
              </ul>
            )}
            <UsageChart
              rows={chart.rows}
              series={chart.series}
              unit={chart.unit}
              stacked={false}
              limit={chart.limit}
              className="h-[140px]"
            />
          </Panel>
        ))
      )}
    </div>
  )
}
