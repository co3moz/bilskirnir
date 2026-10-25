import { test } from "bun:test"
import assert from "node:assert/strict"
import { readdirSync, readFileSync } from "node:fs"

import {
  BRACKET_COLORS,
  diagnosticsOptions,
  editorHeight,
  editorSchema,
  hexColor,
  holdsFixed,
  modelName,
  modelUri,
  monacoLanguage,
  MORE_FILES,
  rowsHeight,
  TOKEN_COLORS,
  tokenRules,
} from "../src/lib/code-editor"
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

test("a file's model ends in its name, so Monaco can tell its language from it", () => {
  assert.equal(modelName("/home/desktop/tools/build.py", undefined), "build.py")
  assert.equal(modelName("/home/desktop/Dockerfile", "bash"), "Dockerfile")
  // Text that is no file says its language, or none.
  assert.equal(modelName(undefined, "json"), "value.json")
  assert.equal(modelName(undefined, "bash"), "script.sh")
  assert.equal(modelName(undefined, undefined), "text.txt")
  assert.ok(modelUri("r1", "build.py").endsWith("/r1/build.py"))
  // A name is one segment of the address, whatever it holds.
  assert.ok(modelUri("r1", "a b#?.sh").endsWith("/r1/a%20b%23%3F.sh"))
  assert.notEqual(modelUri("r1", "x.json"), modelUri("r2", "x.json"))
  assert.equal(monacoLanguage("bash"), "shell")
  assert.equal(monacoLanguage("json"), "json")
})

test("a dialog's translate or a glass surface's blur keeps the editor's widgets inside it", () => {
  const plain = {
    transform: "none",
    translate: "none",
    scale: "none",
    rotate: "none",
    perspective: "none",
    filter: "none",
    backdropFilter: "none",
    contain: "none",
    willChange: "auto",
  }
  assert.equal(holdsFixed(plain), false)
  // Tailwind centers a dialog with the translate property, not a transform.
  assert.equal(holdsFixed({ ...plain, translate: "-50% -50%" }), true)
  assert.equal(holdsFixed({ ...plain, backdropFilter: "blur(24px)" }), true)
  assert.equal(holdsFixed({ ...plain, transform: "matrix(0.95, 0, 0, 0.95, 0, 0)" }), true)
  assert.equal(holdsFixed({ ...plain, contain: "layout paint" }), true)
  assert.equal(holdsFixed({ ...plain, contain: "style" }), false)
  assert.equal(holdsFixed({ ...plain, willChange: "transform" }), true)
  assert.equal(holdsFixed({ ...plain, willChange: "opacity" }), false)
})

test("files Monaco has no language for go to languages it has, without taking another's", () => {
  const definitions = new URL(
    "../node_modules/monaco-editor/esm/vs/languages/definitions/",
    import.meta.url
  )
  const ids = new Set<string>()
  const claimed = new Set<string>()
  for (const folder of readdirSync(definitions, { withFileTypes: true })) {
    if (!folder.isDirectory()) continue
    const source = readFileSync(new URL(`${folder.name}/register.js`, definitions), "utf8")
    for (const [, id] of source.matchAll(/id: "([^"]+)"/g)) ids.add(id)
    for (const [, list] of source.matchAll(/(?:extensions|filenames): \[([^\]]*)\]/g))
      for (const [, name] of list.matchAll(/"([^"]+)"/g)) claimed.add(name.toLowerCase())
  }
  assert.ok(ids.has("shell") && claimed.has(".sh"), "Monaco's own languages are read")
  const added: string[] = []
  for (const files of MORE_FILES) {
    assert.ok(ids.has(files.id), `Monaco has ${files.id}`)
    added.push(...(files.extensions ?? []), ...(files.filenames ?? []))
  }
  // A Monaco that learns one of these, such as TOML, should have it rather than its stand-in.
  for (const name of added) assert.ok(!claimed.has(name.toLowerCase()), `Monaco knows ${name}`)
  assert.equal(new Set(added).size, added.length, "each name goes to one language")
  const language = (extension: string) =>
    MORE_FILES.find((files) => files.extensions?.includes(extension))?.id
  assert.equal(language(".toml"), "ini")
  assert.equal(language(".service"), "ini")
  assert.equal(language(".shader"), "cpp")
  assert.equal(language(".glsl"), "cpp")
})

test("a field of some rows starts as tall as their text", () => {
  assert.equal(rowsHeight(10), 10 * 18 + 16)
})

test("each model is checked against its own schema, and nothing is fetched", () => {
  const first = modelUri("a", "value.json")
  const second = modelUri("b", "value.json")
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
  for (const variable of new Set([...Object.values(TOKEN_COLORS), ...BRACKET_COLORS]))
    assert.ok(css.includes(`${variable}:`), `index.css defines ${variable}`)
  const rules = tokenRules((variable) => (variable === "--code-string" ? "#9fd8a8" : "#ffffff"))
  assert.deepEqual(
    rules.find((rule) => rule.token === "string"),
    { token: "string", foreground: "9fd8a8", fontStyle: "" }
  )
  // Comments lean, as `CodeTextarea` sets them.
  assert.equal(rules.find((rule) => rule.token === "comment")?.fontStyle, "italic")
  // An INI section is a `metatag`, as a `#!` line is, but does not lean as one.
  assert.equal(rules.find((rule) => rule.token === "metatag")?.fontStyle, "italic")
  assert.equal(rules.find((rule) => rule.token === "metatag.ini")?.fontStyle, "")
  // JSON's keys are strings to Monaco, but take the color of a name.
  assert.equal(TOKEN_COLORS["string.key.json"], "--code-variable")
  assert.equal(hexColor([255, 16, 0, 255]), "#ff1000")
  assert.equal(hexColor([0, 0, 0, 64]), "#00000040")
  assert.equal(hexColor([1, 2, 3, 255], false), "010203")
})

test("a field grows with its text between its least and its most", () => {
  assert.equal(editorHeight(10, 36, 240), 36)
  assert.equal(editorHeight(100.4, 36, 240), 100)
  assert.equal(editorHeight(900, 36, 240), 240)
})
