import { test } from "bun:test"
import assert from "node:assert/strict"

import {
  parameterBody,
  readTemplate,
  templateBody,
  templateDraft,
  templateFor,
} from "../web/src/lib/templates"
import type { Template } from "../web/src/lib/types"

const unity: Template = {
  parameters: [
    { name: "UNITY_EMAIL", label: "Unity email", required: true, secret: false, default: "" },
    { name: "UNITY_PASSWORD", label: "", required: true, secret: true, default: "" },
    { name: "BRANCH", label: "Branch", required: false, secret: false, default: "main" },
  ],
  script: "set -eu\n",
  root: false,
  timeout_seconds: 600,
}

const record = (id: string, template?: Template) => ({
  image_id: `sha256:${id}`,
  reference: `bilskirnir/snapshot:${id}`,
  template: template ?? null,
})

test("the template comes from a record of the chosen image, as the manager picks it", () => {
  const records = [record("plain"), record("unity", unity), record("again", unity)]
  assert.equal(templateFor(records, "sha256:unity"), unity)
  assert.equal(templateFor(records, "bilskirnir/snapshot:again"), unity)
  // A profile reference or an image without one asks for nothing.
  assert.equal(templateFor(records, "sha256:plain"), undefined)
  assert.equal(templateFor(records, "ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04"), undefined)
  assert.equal(templateFor(records, undefined), undefined)
  // Two records of one image: the first, newest one with a template wins.
  const shared = [
    { ...record("same"), reference: "a" },
    { ...record("same", { ...unity, root: true }), reference: "b" },
  ]
  assert.equal(templateFor(shared, "sha256:same")?.root, true)
})

test("the create form sends a value for every parameter, the default when left alone", () => {
  assert.deepEqual(parameterBody(unity, {}), {
    UNITY_EMAIL: "",
    UNITY_PASSWORD: "",
    BRANCH: "main",
  })
  assert.deepEqual(parameterBody(unity, { UNITY_EMAIL: "dev@example.com", BRANCH: "" }), {
    UNITY_EMAIL: "dev@example.com",
    UNITY_PASSWORD: "",
    BRANCH: "",
  })
  // Values left from an image chosen earlier are not sent.
  assert.deepEqual(Object.keys(parameterBody(unity, { OTHER: "x" })), [
    "UNITY_EMAIL",
    "UNITY_PASSWORD",
    "BRANCH",
  ])
  assert.deepEqual(parameterBody(undefined, { OTHER: "x" }), {})
})

test("a template survives the editor unchanged, and a secret loses its default", () => {
  assert.deepEqual(templateBody(templateDraft(unity)), unity)
  const draft = templateDraft(unity)
  draft.parameters[2] = { ...draft.parameters[2], secret: true, name: " TOKEN " }
  const body = templateBody(draft)
  assert.deepEqual(body.parameters[2], {
    name: "TOKEN",
    label: "Branch",
    required: false,
    secret: true,
    default: "",
  })
  // A new template starts with a script skeleton and the manager's defaults.
  const fresh = templateDraft()
  assert.deepEqual([fresh.parameters, fresh.root, fresh.timeout_seconds], [[], false, "600"])
  assert.match(fresh.script, /set -euo pipefail/)
})

test("a pasted template fills in what it leaves out and says what is wrong", () => {
  assert.deepEqual(readTemplate({ script: "true", parameters: [{ name: "A" }] }), {
    parameters: [{ name: "A", label: "", required: false, secret: false, default: "" }],
    script: "true",
    root: false,
    timeout_seconds: 600,
  })
  const cases: [unknown, RegExp][] = [
    [null, /JSON object/],
    [{ parameters: [] }, /no setup script/],
    [{ script: "true", parameters: {} }, /must be a list/],
    [{ script: "true", parameters: [{ label: "x" }] }, /needs a name/],
    [{ script: "true", parameters: [{ name: "A", secret: "yes" }] }, /parameter A/],
    [{ script: "true", root: "no" }, /true or false/],
    [{ script: "true", timeout_seconds: "600" }, /number of seconds/],
  ]
  for (const [value, message] of cases)
    assert.throws(() => readTemplate(value), message, JSON.stringify(value))
})
