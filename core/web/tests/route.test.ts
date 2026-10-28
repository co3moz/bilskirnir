import { test } from "bun:test"
import assert from "node:assert/strict"

import { href, parseRoute } from "../src/lib/route"

test("page links survive a reload and unknown pages fall back to the dashboard", () => {
  for (const page of [
    "dashboard",
    "containers",
    "groups",
    "images",
    "templates",
    "volumes",
    "mcp-endpoints",
    "mcp-servers",
    "mcp-inspector",
    "files",
    "jobs",
    "activity",
    "dependencies",
    "settings",
  ] as const)
    assert.deepEqual(parseRoute(href({ page })), { page })
  assert.deepEqual(parseRoute(""), { page: "dashboard" })
  assert.deepEqual(parseRoute("#/nowhere"), { page: "dashboard" })
  assert.deepEqual(parseRoute("#/desktop"), { page: "dashboard" })
})

test("each MCP page has a link under #/mcp, and the MCP page's old link opens the endpoints", () => {
  assert.equal(href({ page: "mcp-endpoints" }), "#/mcp/endpoints")
  assert.equal(href({ page: "mcp-servers" }), "#/mcp/servers")
  assert.equal(href({ page: "mcp-inspector" }), "#/mcp/inspector")
  assert.deepEqual(parseRoute("#/mcp"), { page: "mcp-endpoints" })
  assert.deepEqual(parseRoute("#/mcp/"), { page: "mcp-endpoints" })
  assert.deepEqual(parseRoute("#/mcp/nowhere"), { page: "mcp-endpoints" })
  // The pages' own names are not links of their own.
  assert.deepEqual(parseRoute("#/mcp-servers"), { page: "dashboard" })
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

test("the network groups page keeps the open group in its link", () => {
  assert.deepEqual(parseRoute(href({ page: "groups", group: "build" })), {
    page: "groups",
    group: "build",
  })
  assert.equal(href({ page: "groups", group: "build" }), "#/groups/build")
  assert.equal(href({ page: "groups" }), "#/groups")
  assert.deepEqual(parseRoute("#/groups/"), { page: "groups" })
})

test("the activity page keeps its container, caller and open call in its link", () => {
  const route = {
    page: "activity",
    container: "a1/b2 c3",
    caller: "endpoint:chrome-dev",
    entry: 42,
  } as const
  assert.deepEqual(parseRoute(href(route)), route)
  assert.equal(href({ page: "activity" }), "#/activity")
  assert.deepEqual(parseRoute("#/activity"), { page: "activity" })
  assert.deepEqual(parseRoute("#/activity?caller=administrator"), {
    page: "activity",
    caller: "administrator",
  })
  // A call that is not a positive whole number opens nothing.
  assert.deepEqual(parseRoute("#/activity?entry=x"), { page: "activity" })
  assert.deepEqual(parseRoute("#/activity?entry=-3"), { page: "activity" })
  // Other pages ignore what follows a question mark.
  assert.deepEqual(parseRoute("#/jobs?x=1"), { page: "jobs" })
})
