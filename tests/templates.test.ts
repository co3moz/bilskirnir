import { afterEach, test } from "bun:test"
import assert from "node:assert/strict"

import { dataUrlBytes, iconUrl, readIcon } from "../web/src/lib/icons"
import {
  allTags,
  buildable,
  compareVersions,
  loadTemplate,
  matches,
  readTemplateList,
  templateFile,
} from "../web/src/lib/templates"

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
