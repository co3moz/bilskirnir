import { test } from "bun:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"

import { en } from "../src/locales/en"
import {
  changesOf,
  choiceIndex,
  FIELDS,
  initialText,
  releaseState,
  SECTION_OF,
  sections,
  shownValue,
  textProblem,
} from "../src/lib/settings"
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

test("every control the page has is for a setting the manager lets it change", () => {
  // EDITABLE in src/config.rs is what the manager accepts; a control for anything else would only
  // ever be refused.
  const config = readFileSync(new URL("../../src/config.rs", import.meta.url), "utf8")
  const block = /^pub\(crate\) const EDITABLE: &\[&str\] = &\[\n([\s\S]*?)^\];$/m.exec(config)
  assert.ok(block, "EDITABLE in src/config.rs")
  const editable = [...block[1].matchAll(/^ {4}"([A-Z_]+)",$/gm)].map((match) => match[1])
  for (const name of Object.keys(FIELDS)) assert.ok(editable.includes(name), name)
  for (const field of Object.values(FIELDS))
    if (field.kind === "choice")
      for (const choice of field.choices) if (choice.label) assert.ok(choice.label in en)
})

test("a setting starts from the text its environment gave, or what the manager made of it", () => {
  const variable = (name: string, value: string | number | boolean, text: string) =>
    ({ name, value, editable: true, text }) as ConfigVariable
  assert.equal(initialText(variable("TEMPLATES_URL", "https://wiki/index.json", "")), "")
  assert.equal(initialText(variable("UPLOAD_MAX_MB", 1024, "")), "1024")
  assert.equal(initialText(variable("SECURE_COOKIE", true, "")), "true")
  assert.equal(initialText(variable("SECURE_COOKIE", true, "1")), "1")
  assert.equal(initialText(variable("DESKTOP_IMAGE", "ghcr.io/x/y:1", "")), "ghcr.io/x/y:1")
  assert.equal(initialText(variable("GPU_ADAPTER", "", "")), "")
  assert.equal(initialText({ name: "MCP_TOKEN", secret: true, set: true }), "")
})

test("a draft changes only what differs, and only editable settings", () => {
  const variables: ConfigVariable[] = [
    { name: "SECURE_COOKIE", value: false, editable: true, text: "false" },
    { name: "UPLOAD_MAX_MB", value: 1024, editable: true, text: "1024" },
    { name: "AUTH_MODE", value: "password", editable: false },
    { name: "ADMIN_PASSWORD", secret: true, set: true },
  ]
  assert.deepEqual(
    changesOf(variables, {
      SECURE_COOKIE: "true",
      UPLOAD_MAX_MB: "1024",
      AUTH_MODE: "none",
      ADMIN_PASSWORD: "x",
    }),
    { SECURE_COOKIE: "true" }
  )
})

test("a text is refused before the manager is asked, as .env would misread it", () => {
  for (const text of ["", "true", "GeForce RTX", "https://example.com/index.json", "team=a"])
    assert.equal(textProblem("GPU_ADAPTER", text), null, text)
  for (const text of [
    "$HOME",
    "a#b",
    "'a'",
    '"a"',
    "a`b`",
    String.raw`a\b`,
    " a",
    "a ",
    "a\nb",
    "café",
  ])
    assert.equal(textProblem("GPU_ADAPTER", text)?.key, "settings.edit.problem.text", text)
  assert.equal(textProblem("UPLOAD_MAX_MB", "2048"), null)
  for (const text of ["0", "65537", "1.5", "", "lots"])
    assert.deepEqual(textProblem("UPLOAD_MAX_MB", text), {
      key: "settings.edit.problem.number",
      values: { min: 1, max: 65536 },
    })
})

test("a choice reads its own values and leaves the rest to the user's text", () => {
  const field = FIELDS.TEMPLATES_URL
  assert.ok(field.kind === "choice")
  assert.equal(choiceIndex(field, ""), 0)
  assert.equal(choiceIndex(field, "none"), 1)
  assert.equal(choiceIndex(field, "https://example.com/index.json"), -1)
})
