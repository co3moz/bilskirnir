import { test } from "bun:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"

import {
  followLog,
  jobReadPath,
  LOG_LIMIT,
  logWentBack,
  trimLog,
  type JobRead,
} from "../src/lib/job-log"

const job = (log: string, start: number, end: number, status = "running"): JobRead => ({
  id: "j1",
  kind: "build",
  status: status as JobRead["status"],
  created_at: 1,
  finished_at: status === "running" ? null : 2,
  log,
  log_start: start,
  log_end: end,
})

test("the first read asks for the whole log, later ones from where the last ended", () => {
  assert.equal(jobReadPath("j1", undefined), "jobs/j1?log_offset=0")
  assert.equal(jobReadPath("j1", job("abc", 0, 3)), "jobs/j1?log_offset=3")
})

test("a read that continues the log is joined to it, with the read's own fields", () => {
  const first = followLog(undefined, job("step 1\n", 0, 7))
  assert.equal(first.log, "step 1\n")
  const next = followLog(first, job("step 2\n", 7, 14, "completed"))
  assert.equal(next.log, "step 1\nstep 2\n")
  assert.equal(next.log_end, 14)
  assert.equal(next.status, "completed")
  // Nothing new keeps the log as it was.
  const quiet = followLog(next, job("", 14, 14, "completed"))
  assert.equal(quiet.log, "step 1\nstep 2\n")
  assert.equal(quiet.log_end, 14)
})

test("a read past a gap is the whole log the manager keeps", () => {
  // The bytes between 7 and 300000 were older than the log keeps when it was read.
  const before = job("step 1\n", 0, 7)
  const kept = "x".repeat(LOG_LIMIT)
  const after = followLog(before, job(kept, 300_000 - LOG_LIMIT, 300_000))
  assert.equal(after.log, kept)
})

test("the joined log keeps the last LOG_LIMIT bytes from a character's start", () => {
  assert.equal(trimLog("short"), "short")
  // "é" is two bytes in UTF-8: one more byte than the limit cuts into the first character.
  const log = "é".repeat(LOG_LIMIT / 2) + "a"
  const trimmed = trimLog(log)
  assert.equal(trimmed, "é".repeat(LOG_LIMIT / 2 - 1) + "a")
  assert.ok(new TextEncoder().encode(trimmed).length <= LOG_LIMIT)

  const before = job("a".repeat(LOG_LIMIT), 0, LOG_LIMIT)
  const after = followLog(before, job("bc", LOG_LIMIT, LOG_LIMIT + 2))
  assert.equal(after.log.length, LOG_LIMIT)
  assert.ok(after.log.endsWith("abc"))
})

test("a log whose end went back is read again from its start", () => {
  const before = job("step 1\nstep 2\n", 0, 14)
  // A manager that started the log again from zero now ends it before the drawer's end.
  assert.equal(logWentBack(before, job("", 9, 9)), true)
  assert.equal(logWentBack(before, job("step 3\n", 14, 21)), false)
  assert.equal(logWentBack(undefined, job("step 1\n", 0, 7)), false)
})

test("LOG_LIMIT is the manager's", () => {
  const model = readFileSync(new URL("../../manager/src/model.rs", import.meta.url), "utf8")
  const limit = /const LOG_LIMIT: usize = ([\d_]+);/.exec(model)?.[1]
  assert.ok(limit, "core/manager/src/model.rs must declare LOG_LIMIT")
  assert.equal(Number(limit.replaceAll("_", "")), LOG_LIMIT)
})
