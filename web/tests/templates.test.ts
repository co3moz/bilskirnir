import { afterEach, test } from "bun:test"
import assert from "node:assert/strict"

import { composeSteps } from "../src/lib/compose"
import { dataUrlBytes, iconUrl, readIcon } from "../src/lib/icons"
import {
  allTags,
  buildable,
  compareVersions,
  loadTemplate,
  matches,
  mergeTemplates,
  readTemplateList,
  templateFile,
} from "../src/lib/templates"

const LIST = "https://raw.githubusercontent.com/wiki/co3moz/bilskirnir/templates/index.json"
const unity = {
  id: "unity",
  name: "Unity Hub",
  description: "Unity Hub with a volume for editors.",
  tags: ["games", "3d"],
  requires: "0.5.0",
}

test("a list offers the entries it can and says why it cannot offer the rest", () => {
  const list = readTemplateList({
    templates: [
      unity,
      { id: "vscode", name: "  VS Code  " },
      { id: "Unity", name: "Upper case" },
      { id: "unity", name: "Again" },
      { id: "blank", name: " " },
      { id: "tagged", name: "Tags", tags: Array(9).fill("x") },
      { id: "old", name: "Old", requires: "latest" },
      "not an entry",
    ],
  })
  assert.deepEqual(list.templates, [
    unity,
    { id: "vscode", name: "VS Code", description: "", tags: [] },
  ])
  assert.deepEqual(
    list.broken.map((entry) => entry.id),
    ["Unity", "unity", "blank", "tagged", "old", "#8"]
  )
  assert.match(list.broken[1].reason, /same id/)
  assert.match(list.broken[3].reason, /at most 8/)
  // Fields a newer list adds are ignored rather than refused.
  assert.equal(readTemplateList({ templates: [{ ...unity, stars: 5 }] }).templates.length, 1)
})

test("a list without a templates array is refused as a whole", () => {
  for (const value of [null, [], {}, { templates: {} }, "index"])
    assert.throws(() => readTemplateList(value), /no templates array/, JSON.stringify(value))
})

test("a template needs at least the release it names", () => {
  assert.equal(compareVersions("0.4.0", "0.5.0"), -1)
  assert.equal(compareVersions("0.10.0", "0.9.9"), 1)
  assert.equal(compareVersions("1.0.0", "1.0.0"), 0)
  assert.equal(compareVersions("0.5.0-dev", "0.5.0"), 0)
  assert.ok(buildable(unity, "0.5.0") && buildable(unity, "1.0.0"))
  assert.ok(!buildable(unity, "0.4.9"))
  assert.ok(buildable({}, "0.1.0"))
})

test("a template's files sit beside the list", () => {
  assert.equal(
    templateFile(LIST, "unity", "json"),
    "https://raw.githubusercontent.com/wiki/co3moz/bilskirnir/templates/unity.json"
  )
  assert.equal(
    templateFile("http://10.0.0.5:8000/index.json", "vscode", "png"),
    "http://10.0.0.5:8000/vscode.png"
  )
})

test("search looks at every word in the name, description and tags", () => {
  const template = { ...unity, requires: undefined }
  assert.ok(matches(template, ""))
  assert.ok(matches(template, "unity GAMES"))
  assert.ok(matches(template, "editors 3d"))
  assert.ok(!matches(template, "unity blender"))
  assert.deepEqual(allTags([template, { ...template, tags: ["3d", "art"] }]), [
    "3d",
    "art",
    "games",
  ])
})

const PNG = "data:image/png;base64,"

test("an icon is a PNG data URL the manager accepts", () => {
  assert.equal(dataUrlBytes(`${PNG}QUJD`), 3)
  assert.equal(dataUrlBytes(`${PNG}QUI=`), 2)
  assert.equal(dataUrlBytes(`${PNG}QQ==`), 1)
  assert.equal(readIcon(`${PNG}QUJD`), `${PNG}QUJD`)
  assert.throws(() => readIcon("data:image/webp;base64,UklG"), /PNG data URL/)
  assert.throws(() => readIcon(42), /PNG data URL/)
  assert.throws(() => readIcon(`${PNG}${"A".repeat(410_000)}`), /300 KiB/)
  assert.equal(iconUrl({ id: "a b", icon: "0123abcd" }), "/api/images/a%20b/icon?v=0123abcd")
  assert.equal(iconUrl({ id: "a", icon: null }), undefined)
})

const realFetch = globalThis.fetch
afterEach(() => {
  globalThis.fetch = realFetch
})

/** Answers the template page's requests from a map of URLs, with 404 for the rest. */
function serve(files: Record<string, unknown>) {
  const requested: string[] = []
  globalThis.fetch = (async (input: string | URL | Request, init?: RequestInit) => {
    const url = String(input)
    requested.push(url)
    // The list is public: no request may carry the dashboard's cookies.
    assert.equal(init?.credentials, "omit", url)
    return url in files
      ? new Response(JSON.stringify(files[url]), { status: 200 })
      : new Response("missing", { status: 404 })
  }) as typeof fetch
  return requested
}

test("installing a template fills the build form and names the template", async () => {
  const icon = `${PNG}iVBORw0KGgo=`
  const requested = serve({
    [templateFile(LIST, "unity", "json")]: {
      base_image: "ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04",
      script: "set -eu\n",
      icon,
      template_id: "something-else",
    },
  })
  const config = await loadTemplate(LIST, unity)
  assert.deepEqual(config, {
    base_image: "ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04",
    script: "set -eu\n",
    // Without a name or description of its own, the list's are used.
    name: "Unity Hub",
    description: unity.description,
    // The list's PNG is missing, so the configuration's own icon is the one used.
    icon,
    template_id: "unity",
  })
  assert.deepEqual(requested, [
    templateFile(LIST, "unity", "json"),
    templateFile(LIST, "unity", "png"),
  ])
})

test("a template that cannot fill the build form names itself in the error", async () => {
  serve({ [templateFile(LIST, "unity", "json")]: { name: "No script" } })
  await assert.rejects(
    loadTemplate(LIST, unity),
    /Unity Hub cannot be installed: .*no setup script/
  )
  serve({})
  await assert.rejects(loadTemplate(LIST, unity), /HTTP 404/)
})

const BASE = "ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04"
const step = (name: string, script: string) => ({
  parameters: [
    { name, label: "", description: "", type: "text" as const, required: false },
    { name: "VERSION", label: "", description: "", type: "text" as const, required: false },
  ].map((parameter) => ({ ...parameter, secret: false, default: "" })),
  script,
  root: false,
  timeout_seconds: 600,
})
const chrome = { id: "chrome", name: "Google Chrome", description: "Chrome.", tags: [] }
const vscode = { id: "vscode", name: "VS Code", description: "", tags: [] }

test("merging templates runs each script as a part and each compose as a step", () => {
  const merged = mergeTemplates([
    {
      template: unity,
      config: {
        base_image: BASE,
        script: "install unity",
        internet: true,
        compose: step("WORKSPACE", "setup unity"),
        icon: "data:image/png;base64,AA==",
        template_id: "unity",
      },
    },
    { template: chrome, config: { base_image: BASE, script: "install chrome", internet: false } },
    {
      template: vscode,
      config: {
        base_image: BASE,
        script: "install code",
        internet: false,
        compose: step("GIT_NAME", "setup code"),
      },
    },
  ])
  assert.deepEqual(merged.parts, [
    { id: "unity", name: "Unity Hub", script: "install unity" },
    { id: "chrome", name: "Google Chrome", script: "install chrome" },
    { id: "vscode", name: "VS Code", script: "install code" },
  ])
  // Both composes ask for VERSION, which is fine: each step's script gets its own.
  assert.deepEqual(
    composeSteps(merged.compose!).map((step) => [step.id, step.name, step.script]),
    [
      ["unity", "Unity Hub", "setup unity"],
      ["vscode", "VS Code", "setup code"],
    ]
  )
  assert.equal(merged.name, "Unity Hub + Google Chrome + VS Code")
  assert.equal(merged.description, "Unity Hub with a volume for editors.\n\nChrome.")
  assert.equal(merged.base_image, BASE)
  assert.equal(merged.internet, true)
  assert.equal(merged.icon, "data:image/png;base64,AA==")
  assert.deepEqual(merged.template_ids, ["unity", "chrome", "vscode"])
  assert.equal(merged.warnings, undefined)
})

test("a merge warns about another base and refuses a template it already has", () => {
  const merged = mergeTemplates([
    { template: chrome, config: { base_image: BASE, script: "a" } },
    { template: vscode, config: { base_image: "example/gnome:1", script: "b" } },
  ])
  assert.equal(merged.base_image, BASE)
  assert.match(merged.warnings?.[0] ?? "", /VS Code was made for example\/gnome:1/)
  // A template merged from others brings its parts, so choosing one of them again is refused.
  const both = {
    template: { ...chrome, id: "chrome-and-code", name: "Chrome and Code" },
    config: {
      parts: [
        { id: "chrome", name: "Google Chrome", script: "a" },
        { id: "vscode", name: "VS Code", script: "b" },
      ],
      template_ids: ["chrome", "vscode"],
    },
  }
  assert.throws(
    () => mergeTemplates([both, { template: chrome, config: { script: "a" } }]),
    /Google Chrome is in more than one/
  )
  assert.deepEqual(
    mergeTemplates([both, { template: unity, config: { script: "c" } }]).template_ids,
    ["chrome-and-code", "chrome", "vscode", "unity"]
  )
})

const chromeServer = { name: "chrome", command: "chrome-mcp", args: ["--port", "${PORT}"] }
const xServer = { name: "x", url: "http://127.0.0.1:9000/mcp" }

test("a merge writes every template's desktop MCP servers into the image, each name once", () => {
  const merged = mergeTemplates([
    { template: chrome, config: { script: "a", mcp: [chromeServer] } },
    { template: unity, config: { script: "b" } },
    { template: vscode, config: { script: "c", mcp: [xServer] } },
  ])
  assert.deepEqual(merged.mcp, [chromeServer, xServer])
  assert.equal(
    "mcp" in mergeTemplates([{ template: unity, config: { script: "b" } }]),
    false,
    "a merge without servers declares none"
  )
  assert.throws(
    () =>
      mergeTemplates([
        { template: chrome, config: { script: "a", mcp: [chromeServer] } },
        { template: vscode, config: { script: "c", mcp: [xServer, { ...chromeServer }] } },
      ]),
    /Google Chrome and VS Code both declare the desktop MCP server chrome/
  )
})

test("a list entry names the desktop MCP servers its image declares, and nothing else", () => {
  const [entry] = readTemplateList({
    templates: [{ ...unity, mcp: ["chrome", "x", "Not A Name", 5] }],
  }).templates
  assert.deepEqual(entry.mcp, ["chrome", "x"])
  assert.equal("mcp" in readTemplateList({ templates: [unity] }).templates[0], false)
  // A malformed list of names leaves the card without one, not the template out of the list.
  assert.equal(readTemplateList({ templates: [{ ...unity, mcp: "chrome" }] }).templates.length, 1)
})
