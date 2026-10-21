import { test } from "bun:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"

import {
  diagnosticsOptions,
  editorHeight,
  editorSchema,
  hexColor,
  JSON_TOKENS,
  modelUri,
  tokenRules,
} from "../src/lib/json-editor"
import type { JsonSchema } from "../src/lib/schema-form"

test("a nested field's schema keeps the root's definitions, so its refs resolve", () => {
  const root: JsonSchema = {
    $schema: "https://json-schema.org/draft/2020-12/schema",
    $defs: { point: { type: "object" } },
    definitions: { old: { type: "string" } },
    type: "object",
  }
  const field: JsonSchema = { anyOf: [{ $ref: "#/$defs/point" }, { type: "null" }], nullable: true }
  assert.deepEqual(editorSchema(field, root), {
    $schema: "https://json-schema.org/draft/2020-12/schema",
    $defs: { point: { type: "object" } },
    definitions: { old: { type: "string" } },
    anyOf: [{ $ref: "#/$defs/point" }, { type: "null" }],
  })
  // A root without definitions adds none.
  assert.deepEqual(editorSchema({ type: "array", typeList: ["array"] }, {}), { type: "array" })
})

test("each model is checked against its own schema, and nothing is fetched", () => {
  const first = modelUri("a")
  const second = modelUri("b")
  assert.notEqual(first, second)
  const options = diagnosticsOptions(
    new Map<string, JsonSchema>([
      [first, { type: "object" }],
      [second, { type: "array" }],
    ])
  )
  assert.equal(options.enableSchemaRequest, false)
  assert.equal(options.schemaRequest, "ignore")
  assert.deepEqual(
    options.schemas.map((entry) => [entry.fileMatch, entry.schema.type]),
    [
      [[first], "object"],
      [[second], "array"],
    ]
  )
  assert.equal(new Set(options.schemas.map((entry) => entry.uri)).size, 2)
})

test("the editor's tokens take the code colors the rest of the interface uses", () => {
  const css = readFileSync(new URL("../src/index.css", import.meta.url), "utf8")
  for (const variable of new Set(Object.values(JSON_TOKENS)))
    assert.ok(css.includes(`${variable}:`), `index.css defines ${variable}`)
  const rules = tokenRules((variable) => (variable === "--code-string" ? "#9fd8a8" : "#ffffff"))
  assert.deepEqual(
    rules.find((rule) => rule.token === "string.value.json"),
    { token: "string.value.json", foreground: "9fd8a8" }
  )
  assert.equal(hexColor([255, 16, 0, 255]), "#ff1000")
  assert.equal(hexColor([0, 0, 0, 64]), "#00000040")
  assert.equal(hexColor([1, 2, 3, 255], false), "010203")
})

test("a field grows with its text between its least and its most", () => {
  assert.equal(editorHeight(10, 36, 240), 36)
  assert.equal(editorHeight(100.4, 36, 240), 100)
  assert.equal(editorHeight(900, 36, 240), 240)
})
