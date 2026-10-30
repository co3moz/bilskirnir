import { test } from "bun:test"
import assert from "node:assert/strict"

import {
  draftOf,
  fieldKind,
  formDraft,
  normalizeSchema,
  objectVariants,
  readForm,
  switchVariant,
  typeLabel,
  type Draft,
  type JsonSchema,
} from "../src/lib/schema-form"

const actions: JsonSchema = {
  type: "object",
  properties: {
    url: { type: "string" },
    tries: { type: "integer", minimum: 1 },
    ratio: { type: "number" },
    headless: { type: "boolean" },
    verbose: { type: "boolean" },
    mode: { enum: ["fast", "safe"] },
    kind: { const: "batch" },
    viewport: {
      type: "object",
      properties: { width: { type: "integer" }, height: { type: "integer" } },
      required: ["width", "height"],
    },
    tags: { type: "array", items: { type: "string" } },
    steps: {
      type: "array",
      items: {
        oneOf: [
          {
            type: "object",
            properties: { type: { const: "click" }, x: { type: "number" }, y: { type: "number" } },
            required: ["type", "x", "y"],
          },
          {
            type: "object",
            properties: { type: { const: "type" }, text: { type: "string" } },
            required: ["type", "text"],
          },
        ],
      },
    },
    extra: { type: ["string", "number"] },
    options: { $ref: "#/$defs/options" },
  },
  required: ["url", "headless", "kind"],
  $defs: { options: { type: "object", properties: { retry: { type: "boolean" } } } },
}

test("a value goes into the form and comes out as the same arguments", () => {
  const value = {
    url: "https://example.com",
    tries: 3,
    ratio: 0.5,
    headless: true,
    verbose: false,
    mode: "safe",
    kind: "batch",
    viewport: { width: 1280, height: 720 },
    tags: ["a", "b"],
    steps: [
      { type: "click", x: 10, y: 20 },
      { type: "type", text: "hello" },
    ],
    extra: 5,
    options: { retry: true },
  }
  const { value: read, errors } = readForm(actions, formDraft(actions, value))
  assert.deepEqual(errors, [])
  assert.deepEqual(read, value)
})

test("fields left empty are left out, and required ones say so by their path", () => {
  const { value, errors } = readForm(actions, formDraft(actions))
  // A required box left off is false, a constant of one value is set by itself.
  assert.deepEqual(value, { headless: false, kind: "batch" })
  assert.deepEqual(
    errors.map((error) => error.path),
    ["url"]
  )
})

test("an optional object is sent only once something in it is filled in", () => {
  const fields = formDraft(actions, { url: "x", viewport: { width: 800 } })
  const { errors } = readForm(actions, fields)
  assert.deepEqual(
    errors.map((error) => error.path),
    ["viewport.height"]
  )
})

test("typed text is checked against its type", () => {
  const fields = formDraft(actions, { url: "x" })
  fields.tries = { kind: "text", text: "1.5" }
  fields.ratio = { kind: "text", text: "half" }
  const { errors } = readForm(actions, fields)
  assert.deepEqual(
    errors.map((error) => error.path),
    ["tries", "ratio"]
  )
})

test("array items report errors by their index", () => {
  const fields = formDraft(actions, {
    url: "x",
    steps: [{ type: "click", x: 1, y: 2 }, { type: "type" }],
  })
  const { errors } = readForm(actions, fields)
  assert.deepEqual(
    errors.map((error) => error.path),
    ["steps[1].text"]
  )
})

test("a union of objects is told apart by the property every variant fixes", () => {
  const steps = normalizeSchema(actions.properties!.steps.items, actions)
  const choice = objectVariants(steps, actions)
  assert.equal(choice?.key, "type")
  assert.equal(choice?.variants.length, 2)
  // Choosing the other variant keeps no field of the first one's that it does not have.
  const draft = draftOf(steps, actions, 0, { type: "click", x: 1, y: 2 }, true) as Extract<
    Draft,
    { kind: "union" }
  >
  assert.equal(draft.variant, 0)
  const switched = switchVariant(steps, actions, 0, draft, 1) as Extract<Draft, { kind: "union" }>
  assert.equal(switched.variant, 1)
  assert.deepEqual(Object.keys(switched.fields), ["text"])
})

test("a union of plain values is a JSON field, and text that is not JSON is a string", () => {
  const extra = normalizeSchema(actions.properties!.extra, actions)
  assert.equal(fieldKind(extra, actions, 0), "json")
  const fields = formDraft(actions, { url: "x" })
  fields.extra = { kind: "text", text: "not json" }
  assert.equal(readForm(actions, fields).value.extra, "not json")
  // An array must be JSON.
  const list: JsonSchema = {
    type: "object",
    properties: { points: { type: "array", items: { type: "array" } } },
  }
  const drafts = formDraft(list)
  drafts.points = { kind: "text", text: "[1," }
  assert.equal(readForm(list, drafts).errors[0]?.path, "points")
})

test("nullable types and unions with null take the field of the other type", () => {
  const schema = normalizeSchema({ anyOf: [{ type: "string" }, { type: "null" }] }, {})
  assert.equal(schema.type, "string")
  assert.equal(schema.nullable, true)
  assert.equal(typeLabel(normalizeSchema({ type: ["integer", "null"] }, {}), {}), "integer | null")
  const choices = normalizeSchema({ oneOf: [{ const: "a" }, { const: "b" }] }, {})
  assert.deepEqual(choices.enum, ["a", "b"])
})

test("refs resolve, keeping the keywords beside them", () => {
  const root: JsonSchema = {
    $defs: { name: { type: "string", description: "From the definition" } },
  }
  const schema = normalizeSchema({ $ref: "#/$defs/name", description: "Beside the ref" }, root)
  assert.equal(schema.type, "string")
  assert.equal(schema.description, "Beside the ref")
  // A ref to nowhere, or a loop, stops.
  assert.deepEqual(normalizeSchema({ $ref: "#/$defs/missing" }, root), { $ref: "#/$defs/missing" })
  const loop: JsonSchema = { $defs: { a: { $ref: "#/$defs/a" } } }
  assert.doesNotThrow(() => normalizeSchema({ $ref: "#/$defs/a" }, loop))
})

test("a draft of another kind, as after the server changed the schema, reads as empty", () => {
  const schema: JsonSchema = {
    type: "object",
    properties: { size: { type: "integer" } },
    required: ["size"],
  }
  const { errors } = readForm(schema, { size: { kind: "boolean", value: true } })
  assert.deepEqual(
    errors.map((error) => error.path),
    ["size"]
  )
})

test("a schema nested past the depth limit is edited as JSON", () => {
  let schema: JsonSchema = { type: "string" }
  for (let level = 0; level < 8; level++)
    schema = { type: "object", properties: { inner: schema }, required: ["inner"] }
  const root: JsonSchema = { type: "object", properties: { deep: schema } }
  let node = normalizeSchema(root.properties!.deep, root)
  let depth = 0
  while (fieldKind(node, root, depth) === "object") {
    node = normalizeSchema(node.properties!.inner, root)
    depth++
  }
  assert.equal(fieldKind(node, root, depth), "json")
  assert.ok(depth <= 6, String(depth))
})

test("a type that refers to itself in its own union, as a nullable tree does, stops", () => {
  // As a desktop server may send it: a field of this type is edited as JSON, not recursed into.
  const root: JsonSchema = {
    type: "object",
    properties: { node: { $ref: "#/$defs/a" } },
    $defs: { a: { anyOf: [{ $ref: "#/$defs/a" }, { type: "null" }] } },
  }
  const schema = normalizeSchema({ $ref: "#/$defs/a" }, root)
  assert.equal(schema.nullable, true)
  assert.equal(fieldKind(schema, root, 0), "json")
  const form = formDraft(root)
  assert.deepEqual(readForm(root, form).errors, [])
  // Two definitions that hold each other in their unions stop as well.
  const pair: JsonSchema = {
    $defs: {
      a: { oneOf: [{ $ref: "#/$defs/b" }, { type: "string" }] },
      b: { oneOf: [{ $ref: "#/$defs/a" }, { type: "integer" }] },
    },
  }
  assert.doesNotThrow(() => normalizeSchema({ $ref: "#/$defs/a" }, pair))
})
