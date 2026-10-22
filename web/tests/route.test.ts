import { test } from "bun:test"
import assert from "node:assert/strict"

import { href, parseRoute } from "../src/lib/route"

test("page links survive a reload and unknown pages fall back to the dashboard", () => {
  for (const page of [
    "dashboard",
    "containers",
    "images",
    "templates",
    "volumes",
    "mcp",
    "files",
    "jobs",
    "settings",
  ] as const)
    assert.deepEqual(parseRoute(href({ page })), { page })
  assert.deepEqual(parseRoute(""), { page: "dashboard" })
  assert.deepEqual(parseRoute("#/nowhere"), { page: "dashboard" })
  assert.deepEqual(parseRoute("#/desktop"), { page: "dashboard" })
})

test("a desktop link carries its container ID intact", () => {
  const id = "a1b2/c3 d4#e5"
  assert.deepEqual(parseRoute(href({ page: "desktop", id })), { page: "desktop", id })
})

test("the files page keeps the chosen container in its link", () => {
  const container = "a1b2/c3 d4#e5"
  assert.deepEqual(parseRoute(href({ page: "files", container })), { page: "files", container })
  assert.deepEqual(parseRoute("#/files"), { page: "files" })
  assert.deepEqual(parseRoute("#/files/"), { page: "files" })
  assert.equal(href({ page: "files" }), "#/files")
})
