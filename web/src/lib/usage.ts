import { useSyncExternalStore } from "react"

import { api } from "./api"

// The JSON of GET /api/usage; see "Resource usage" in docs/API.md.
export type UsageSample = {
  t: number
  /** CPU cores in use, measured against the host's CPUs. */
  cpu: number
  memory: number
  disk_read: number
  disk_write: number
  net_rx: number
  net_tx: number
}
export type UsageSeries = {
  id: string
  name: string
  cpu_limit: number | null
  memory_limit: number | null
  online_cpus: number | null
  samples: UsageSample[]
}
export type GpuSample = { t: number; utilization: number | null; memory: number | null }
export type GpuDevice = {
  index: number
  name: string
  memory_total: number | null
  samples: GpuSample[]
}
export type GpuState = "disabled" | "idle" | "sampling" | "unavailable"
export type UsageReport = {
  available: boolean
  error: string | null
  interval_ms: number
  window_ms: number
  cursor: number
  containers: UsageSeries[]
  gpu: { state: GpuState; reason: string | null; scope: "host"; devices: GpuDevice[] }
}

/** What the dashboard holds: the last 30 minutes, grown by each response after the first. */
export type UsageHistory = Omit<UsageReport, "interval_ms"> & {
  /** The newest sample's time; the charts end here rather than at the browser's clock. */
  latest: number
}

export const usageUrl = (since = 0) => (since > 0 ? `usage?since=${since}` : "usage")

const newest = (samples: { t: number }[]) => samples[samples.length - 1]?.t ?? 0

/** Append what came after `before` and forget what fell out of the window. */
function grow<T extends { t: number }>(before: T[] | undefined, added: T[], oldest: number) {
  const last = before ? newest(before) : 0
  const kept = (before ?? []).filter((sample) => sample.t >= oldest)
  return kept.concat(added.filter((sample) => sample.t > last && sample.t >= oldest))
}

/** The history after one response. A container the response no longer lists was deleted. */
export function mergeUsage(history: UsageHistory | null, report: UsageReport): UsageHistory {
  const latest = Math.max(
    history?.latest ?? 0,
    ...report.containers.map((series) => newest(series.samples)),
    ...report.gpu.devices.map((device) => newest(device.samples))
  )
  const oldest = latest - report.window_ms
  const previous = new Map(history?.containers.map((series) => [series.id, series]))
  const devices = new Map(history?.gpu.devices.map((device) => [device.index, device]))
  return {
    available: report.available,
    error: report.error,
    window_ms: report.window_ms,
    cursor: report.cursor,
    latest,
    containers: report.containers.map((series) => ({
      ...series,
      samples: grow(previous.get(series.id)?.samples, series.samples, oldest),
    })),
    gpu: {
      ...report.gpu,
      devices: report.gpu.devices.map((device) => ({
        ...device,
        samples: grow(devices.get(device.index)?.samples, device.samples, oldest),
      })),
    },
  }
}

export type Metric = "cpu" | "memory" | "disk" | "network"
export const metricValue: Record<Metric, (sample: UsageSample) => number> = {
  cpu: (sample) => sample.cpu,
  memory: (sample) => sample.memory,
  disk: (sample) => sample.disk_read + sample.disk_write,
  network: (sample) => sample.net_rx + sample.net_tx,
}

/** The time ranges the charts offer, with the step that keeps each near 300 points. */
export const ranges = [
  { minutes: 5, step: 1000 },
  { minutes: 15, step: 3000 },
  { minutes: 30, step: 6000 },
] as const
export type RangeMinutes = (typeof ranges)[number]["minutes"]
export const rangeStep = (minutes: RangeMinutes) =>
  ranges.find((range) => range.minutes === minutes)?.step ?? 1000

/** A missing step this close to the last sample holds its value; a gap from timing jitter is not a drop to zero. */
const HOLD = 2

export type Row = { t: number } & Record<string, number>

/**
 * One row per step of the range, ending at `end`: each key's average in that step, its last
 * value through a short gap, and 0 where it has nothing, as a stopped container uses nothing.
 * Every key appears in every row, so stacked areas line up, and series that share a key add up.
 */
export function stepRows<T extends { t: number }>(
  series: { key: string; samples: T[]; value: (sample: T) => number | null }[],
  end: number,
  minutes: number,
  step: number
): Row[] {
  const last = Math.floor(end / step) * step
  const count = Math.round((minutes * 60_000) / step)
  const first = last - (count - 1) * step
  const rows: Row[] = Array.from({ length: count }, (_, index) => ({ t: first + index * step }))
  for (const { key, samples, value } of series) {
    const sums = new Array<number>(count).fill(0)
    const counts = new Array<number>(count).fill(0)
    for (const sample of samples) {
      const index = Math.floor((sample.t - first) / step)
      const measured = value(sample)
      if (index < 0 || index >= count || measured === null) continue
      sums[index] += measured
      counts[index] += 1
    }
    let held = 0
    let since = Infinity
    for (let index = 0; index < count; index++) {
      if (counts[index]) {
        held = sums[index] / counts[index]
        since = 0
      } else {
        since += 1
      }
      rows[index][key] = (rows[index][key] ?? 0) + (since <= HOLD ? held : 0)
    }
  }
  return rows
}

/**
 * Axis ticks from 0 to at least `top` in clean steps: 1, 2, 2.5 or 5 times a power of ten. Byte
 * figures step in the binary unit their top falls in, so they read 200 MiB rather than 190.7 MiB.
 */
export function niceTicks(top: number, bytes = false, count = 4): number[] {
  let unit = 1
  if (bytes) while (top / unit >= 1024 && unit < 1024 ** 4) unit *= 1024
  const rough = top / unit / (count - 1)
  if (!(rough > 0)) return [0, unit]
  const power = 10 ** Math.floor(Math.log10(rough))
  const step = ([1, 2, 2.5, 5].find((factor) => factor * power >= rough) ?? 10) * power
  const steps = Math.ceil(top / unit / step - 1e-9)
  return Array.from(
    { length: steps + 1 },
    (_, index) => Number((index * step).toPrecision(12)) * unit
  )
}

/** The colors the charts give containers, each a CSS variable from web/src/index.css. */
export const SERIES_SLOTS = 5
export const slotColor = (slot: number | undefined) =>
  slot === undefined ? "var(--series-other)" : `var(--series-${slot + 1})`

/**
 * Give each container a color slot and keep it while the container is listed, so a new or
 * departed container never repaints the others. Containers past the last slot share "Other".
 */
export function assignSlots(previous: ReadonlyMap<string, number>, ids: string[]) {
  const slots = new Map([...previous].filter(([id]) => ids.includes(id)))
  const taken = new Set(slots.values())
  for (const id of ids) {
    if (slots.has(id)) continue
    const free = Array.from({ length: SERIES_SLOTS }, (_, slot) => slot).find(
      (slot) => !taken.has(slot)
    )
    if (free === undefined) break
    slots.set(id, free)
    taken.add(free)
  }
  return slots
}

/** Containers in the order they first reported, so the longest running keep the first colors. */
export const byFirstSample = (containers: UsageSeries[]) =>
  [...containers].sort(
    (a, b) =>
      (a.samples[0]?.t ?? Infinity) - (b.samples[0]?.t ?? Infinity) || a.id.localeCompare(b.id)
  )

// One poller for every chart on the page: it asks for what came after its cursor once a second
// while a chart is shown and the tab is visible.
const INTERVAL = 1000
type State = { history: UsageHistory | null; error: string | null; slots: Map<string, number> }
let state: State = { history: null, error: null, slots: new Map() }
const listeners = new Set<() => void>()
let timer: ReturnType<typeof setTimeout> | undefined
let polling = false

const visible = () => typeof document === "undefined" || document.visibilityState === "visible"

function schedule() {
  clearTimeout(timer)
  timer = listeners.size && visible() ? setTimeout(poll, INTERVAL) : undefined
}

async function poll() {
  if (polling) return
  polling = true
  try {
    const report = await api<UsageReport>(usageUrl(state.history?.cursor))
    const history = mergeUsage(state.history, report)
    const ids = byFirstSample(history.containers).map((series) => series.id)
    state = { history, error: null, slots: assignSlots(state.slots, ids) }
  } catch (error) {
    state = { ...state, error: (error as Error).message }
  } finally {
    polling = false
    listeners.forEach((listener) => listener())
    schedule()
  }
}

if (typeof document !== "undefined") {
  document.addEventListener("visibilitychange", () => {
    if (listeners.size && visible()) void poll()
    else clearTimeout(timer)
  })
}

function subscribe(listener: () => void) {
  listeners.add(listener)
  if (listeners.size === 1) void poll()
  return () => {
    listeners.delete(listener)
    if (!listeners.size) clearTimeout(timer)
  }
}

/** The usage history, kept current while any component reads it. */
export const useUsage = () => useSyncExternalStore(subscribe, () => state)
