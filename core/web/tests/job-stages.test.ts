import { test } from "bun:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"

import { currentStage, jobStages } from "../src/lib/format"
import type { JobStage, JobStatus } from "../src/lib/types"
import { en } from "../src/locales/en"

const plan = (...times: [number?, number?][]): JobStage[] =>
  ["base", "script", "layer", "image", "cleanup"].map((name, index) => {
    const [started_at, finished_at] = times[index] ?? []
    return { name, started_at, finished_at }
  })

const states = (status: JobStatus, stages: JobStage[], finished_at: number | null, now = 0) =>
  jobStages({ status, stages, finished_at }, now).rows.map((row) => [row.state, row.seconds])

test("a running build counts its current stage up to now and waits on the rest", () => {
  const stages = plan([100, 102], [102, 352], [352])
  assert.deepEqual(states("running", stages, null, 400), [
    ["done", 2],
    ["done", 250],
    ["running", 48],
    ["pending", null],
    ["pending", null],
  ])
  assert.equal(jobStages({ status: "running", stages, finished_at: null }, 400).total, 300)
  assert.equal(currentStage({ status: "running", stages }), "Saving the image layer")
  assert.equal(currentStage({ status: "completed", stages }), undefined)
  assert.equal(currentStage({ status: "running" }), undefined)
})

test("a finished build shows each stage's time and the whole run's", () => {
  // Without BuildKit's markers the layer stage never starts, and it was skipped.
  const stages = plan([100, 102], [102, 420], [], [420, 421], [421, 421])
  assert.deepEqual(states("completed", stages, 421, 9999), [
    ["done", 2],
    ["done", 318],
    ["skipped", null],
    ["done", 1],
    ["done", 0],
  ])
  assert.equal(jobStages({ status: "completed", stages, finished_at: 421 }, 9999).total, 321)
})

test("a failed or interrupted build stops at its last started stage", () => {
  const stages = plan([100, 102], [102, 130])
  for (const status of ["failed", "interrupted"] as const)
    assert.deepEqual(states(status, stages, 130), [
      ["done", 2],
      ["failed", 28],
      ["pending", null],
      ["pending", null],
      ["pending", null],
    ])
  // An interruption recorded before stages were closed on restart counts to the job's end.
  assert.deepEqual(states("interrupted", plan([100, 102], [102]), 160)[1], ["failed", 58])
})

test("a job without stages lists none", () => {
  assert.deepEqual(jobStages({ status: "running", finished_at: null }, 5), {
    rows: [],
    total: null,
  })
})

test("a settings change of a stopped container skips stopping it while it commits", () => {
  const stages: JobStage[] = [
    { name: "stop" },
    { name: "commit", started_at: 100 },
    { name: "create" },
    { name: "cleanup" },
  ]
  assert.deepEqual(states("running", stages, null, 160), [
    ["skipped", null],
    ["running", 60],
    ["pending", null],
    ["pending", null],
  ])
  assert.equal(currentStage({ status: "running", stages }), "Committing the container layer")
})

// The manager names the stages of every job that reports them; each needs words in the
// interface, the same words its log uses, and words for a stage the manager no longer reports
// are left over.
test("the interface names exactly the stages the manager reports", () => {
  const source = readFileSync(new URL("../../manager/src/stages.rs", import.meta.url), "utf8")
  const list = (name: string) => {
    const items = new RegExp(String.raw`const ${name}\b[^=]*=\s*\[([\s\S]*?)\];`).exec(source)?.[1]
    assert.ok(items, `core/manager/src/stages.rs must declare ${name}`)
    return items
  }
  const reported = new Map<string, string>()
  for (const [, job] of list("JOB_STAGES").matchAll(/&(\w+)/g))
    for (const [, name, line] of list(job).matchAll(/\("(\w+)",\s*"([^"]*)"\)/g))
      reported.set(name, line)
  assert.ok(reported.has("base") && reported.has("commit"), [...reported.keys()].join())
  const named = Object.entries(en)
    .filter(([key]) => key.startsWith("stage."))
    .map(([key, text]) => [key.slice("stage.".length), text])
  assert.deepEqual(named.sort(), [...reported].sort())
})
