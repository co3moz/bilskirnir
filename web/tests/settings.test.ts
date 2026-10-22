import { test } from "bun:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"

import { en } from "../src/locales/en"
import { releaseState, SECTION_OF, sections, shownValue } from "../src/lib/settings"
import type { ConfigVariable } from "../src/lib/types"

test("every setting the page knows is still an installation setting and says what it does", () => {
  const example = readFileSync(new URL("../../.env.example", import.meta.url), "utf8")
  for (const name of Object.keys(SECTION_OF)) {
    assert.ok(example.includes(`\n${name}=`), `.env.example no longer has ${name}`)
    assert.ok(`settings.about.${name}` in en, `settings.about.${name} is missing`)
  }
})

test("settings are listed by section in the manager's order, unknown ones last", () => {
  const variables: ConfigVariable[] = [
    { name: "UPDATE_CHECK", value: true },
    { name: "AUTH_MODE", value: "password" },
    { name: "ADMIN_PASSWORD", secret: true, set: true },
    { name: "SOMETHING_NEWER", value: "x" },
    { name: "ROOT_MCP_ENABLED", value: false },
  ]
  assert.deepEqual(
    sections(variables).map(({ section, variables }) => [
      section,
      variables.map((variable) => variable.name),
    ]),
    [
      ["access", ["AUTH_MODE", "ADMIN_PASSWORD"]],
      ["mcp", ["ROOT_MCP_ENABLED"]],
      ["manager", ["UPDATE_CHECK"]],
      ["misc", ["SOMETHING_NEWER"]],
    ]
  )
})

test("a secret shows only whether it is set, and empty values read as not set", () => {
  assert.deepEqual(shownValue({ name: "MCP_TOKEN", secret: true, set: false }), {
    kind: "secret",
    set: false,
  })
  assert.deepEqual(shownValue({ name: "GPU_ADAPTER", value: "" }), { kind: "empty" })
  assert.deepEqual(shownValue({ name: "MANAGER_CONTAINER", value: null }), { kind: "empty" })
  assert.deepEqual(shownValue({ name: "WSL_GPU", value: false }), { kind: "text", text: "false" })
  assert.deepEqual(shownValue({ name: "UPLOAD_MAX_MB", value: 1024 }), {
    kind: "text",
    text: "1024",
  })
})

test("the release state follows the checks, a running update and the last answer", () => {
  const status = { enabled: true, job_id: null, available: false, newest: null, error: null }
  assert.equal(releaseState({ ...status, enabled: false, available: true }), "off")
  assert.equal(releaseState(status), "unchecked")
  assert.equal(releaseState({ ...status, error: "GitHub is unreachable" }), "failed")
  assert.equal(releaseState({ ...status, newest: "0.6.2" }), "current")
  // A failed look keeps the release seen before.
  assert.equal(releaseState({ ...status, newest: "0.6.2", error: "rate limited" }), "current")
  assert.equal(releaseState({ ...status, newest: "0.7.0", available: true }), "available")
  assert.equal(
    releaseState({ ...status, newest: "0.7.0", available: true, job_id: "j" }),
    "updating"
  )
})
