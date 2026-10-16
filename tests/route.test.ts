import { test } from "bun:test"
import assert from "node:assert/strict"

import { href, parseRoute } from "../web/src/lib/route"

test("page links survive a reload and unknown pages fall back to the dashboard", () => {
  for (const page of ["dashboard", "containers", "images", "volumes", "jobs"] as const)
    assert.deepEqual(parseRoute(href({ page })), { page })
  assert.deepEqual(parseRoute(""), { page: "dashboard" })
  assert.deepEqual(parseRoute("#/settings"), { page: "dashboard" })
  assert.deepEqual(parseRoute("#/desktop"), { page: "dashboard" })
})

test("a desktop link carries its container ID intact", () => {
  const id = "a1b2/c3 d4#e5"
  assert.deepEqual(parseRoute(href({ page: "desktop", id })), { page: "desktop", id })
})
