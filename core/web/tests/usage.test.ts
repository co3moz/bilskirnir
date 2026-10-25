import { test } from "bun:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"

import {
  SERIES_SLOTS,
  assignSlots,
  byFirstSample,
  mergeUsage,
  niceTicks,
  ranges,
  stepRows,
  usageUrl,
  type UsageReport,
  type UsageSample,
  type UsageSeries,
} from "../src/lib/usage"

const sample = (t: number, cpu = 1): UsageSample => ({
  t,
  cpu,
  memory: 100,
  disk_read: 0,
  disk_write: 0,
  net_rx: 0,
  net_tx: 0,
})
const series = (id: string, samples: UsageSample[], removed = false): UsageSeries => ({
  id,
  name: id,
  cpu_limit: null,
  removed,
  memory_limit: 1000,
  online_cpus: 8,
  samples,
})
const report = (cursor: number, containers: UsageSeries[]): UsageReport => ({
  available: true,
  error: null,
  interval_ms: 1000,
  window_ms: 60_000,
  cursor,
  containers,
  gpu: { state: "idle", reason: null, scope: "host", devices: [] },
})

test("the first request asks for everything and later ones for what came after", () => {
  assert.equal(usageUrl(), "usage")
  assert.equal(usageUrl(0), "usage")
  assert.equal(usageUrl(1792143000123), "usage?since=1792143000123")
})

test("responses grow the history and a container the manager stops listing goes", () => {
  let history = mergeUsage(
    null,
    report(2000, [series("a", [sample(1000), sample(2000)]), series("b", [sample(1500)])])
  )
  assert.equal(history.latest, 2000)
  // A sample already held is not added twice, and b has nothing new but stays.
  history = mergeUsage(
    history,
    report(3000, [series("a", [sample(2000), sample(3000)]), series("b", [])])
  )
  assert.deepEqual(
    history.containers.map((entry) => [entry.id, entry.samples.map((s) => s.t)]),
    [
      ["a", [1000, 2000, 3000]],
      ["b", [1500]],
    ]
  )
  // Deleted: still listed, so its history stays and says so.
  history = mergeUsage(history, report(3500, [series("a", [sample(3500)]), series("b", [], true)]))
  assert.deepEqual(
    history.containers.map((entry) => [entry.id, entry.removed, entry.samples.length]),
    [
      ["a", false, 4],
      ["b", true, 1],
    ]
  )
  // Aged out: no longer listed.
  history = mergeUsage(history, report(4000, [series("a", [sample(4000)])]))
  assert.deepEqual(
    history.containers.map((entry) => entry.id),
    ["a"]
  )
  // Samples older than the window fall out.
  history = mergeUsage(history, report(70_000, [series("a", [sample(62_000)])]))
  assert.deepEqual(
    history.containers[0].samples.map((s) => s.t),
    [2000, 3000, 3500, 4000, 62_000]
  )
  assert.equal(history.cursor, 70_000)
})

test("steps average their samples, hold through jitter and drop to zero when stopped", () => {
  const rows = stepRows(
    [
      {
        key: "a",
        // Two samples in the 5 s step, none in the 6 s step, then the container stops.
        samples: [sample(3000, 1), sample(5000, 1), sample(5900, 3)],
        value: (s) => s.cpu,
      },
    ],
    9500,
    10 / 60,
    1000
  )
  assert.equal(rows.length, 10)
  assert.deepEqual(
    rows.map((row) => row.t),
    [0, 1000, 2000, 3000, 4000, 5000, 6000, 7000, 8000, 9000]
  )
  assert.deepEqual(
    rows.map((row) => row.a),
    [0, 0, 0, 1, 1, 2, 2, 2, 0, 0]
  )
})

test("series that share a key add up, as the containers folded into Other do", () => {
  const rows = stepRows(
    [
      { key: "other", samples: [sample(1000, 0.5)], value: (s) => s.cpu },
      { key: "other", samples: [sample(1200, 0.25)], value: (s) => s.cpu },
    ],
    1000,
    1 / 60,
    1000
  )
  assert.equal(rows.length, 1)
  assert.equal(rows[0].other, 0.75)
  // A figure the driver did not report is skipped rather than averaged in as zero.
  const gpu = stepRows(
    [
      {
        key: "gpu",
        samples: [
          { t: 1000, utilization: 40 },
          { t: 1500, utilization: null },
        ],
        value: (s) => s.utilization,
      },
    ],
    1000,
    1 / 60,
    1000
  )
  assert.equal(gpu[0].gpu, 40)
})

test("each range draws about 300 points", () => {
  for (const range of ranges) {
    assert.equal((range.minutes * 60_000) / range.step, 300, `${range.minutes}`)
  }
})

test("a container keeps its color while others come and go", () => {
  const ids = ["a", "b", "c"]
  let slots = assignSlots(new Map(), ids)
  assert.deepEqual(
    [...slots],
    [
      ["a", 0],
      ["b", 1],
      ["c", 2],
    ]
  )
  // b goes: a and c keep theirs, and a new container takes the free color.
  slots = assignSlots(slots, ["a", "c", "d"])
  assert.deepEqual([...slots].sort(), [
    ["a", 0],
    ["c", 2],
    ["d", 1],
  ])
  // Past the last color, containers are left to share "Other".
  const many = Array.from({ length: SERIES_SLOTS + 2 }, (_, index) => `x${index}`)
  slots = assignSlots(new Map(), many)
  assert.equal(slots.size, SERIES_SLOTS)
  assert.equal(slots.has(`x${SERIES_SLOTS}`), false)
})

test("the longest running containers come first", () => {
  const sorted = byFirstSample([
    series("new", [sample(5000)]),
    series("idle", []),
    series("old", [sample(1000)]),
  ])
  assert.deepEqual(
    sorted.map((entry) => entry.id),
    ["old", "new", "idle"]
  )
})

test("the routes the dashboard reads are served and every series color is defined", () => {
  const routes = readFileSync(new URL("../../manager/src/main.rs", import.meta.url), "utf8")
  for (const route of ['"/api/usage"', '"/api/containers/{id}/usage"']) {
    assert.ok(routes.includes(`.route(${route},`), `core/manager/src/main.rs must serve ${route}`)
  }
  const css = readFileSync(new URL("../src/index.css", import.meta.url), "utf8")
  for (const name of [
    ...Array.from({ length: SERIES_SLOTS }, (_, slot) => `--series-${slot + 1}:`),
    "--series-other:",
  ]) {
    // Once for the light theme and once for the dark one.
    assert.equal(css.split(name).length - 1, 2, name)
  }
})

test("axes step cleanly, in binary units for bytes", () => {
  const MiB = 1024 ** 2
  assert.deepEqual(niceTicks(0.93), [0, 0.5, 1])
  assert.deepEqual(niceTicks(100), [0, 50, 100])
  assert.deepEqual(
    niceTicks(528.9 * MiB, true),
    [0, 200, 400, 600].map((value) => value * MiB)
  )
  assert.deepEqual(niceTicks(8 * MiB, true), [0, 5 * MiB, 10 * MiB])
  assert.deepEqual(niceTicks(3000, true), [0, 1024, 2048, 3072])
  // Nothing measured still gives a scale.
  assert.deepEqual(niceTicks(0), [0, 1])
})
