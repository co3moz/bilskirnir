import { test } from "bun:test"
import assert from "node:assert/strict"

import {
  displayOf,
  draftParameter,
  newOption,
  parameterBody,
  readCompose,
  sliderValue,
  composeBody,
  composeDraft,
  composeFor,
  toggleChoice,
  withOptions,
  withType,
} from "../web/src/lib/compose"
import type { Compose, ComposeParameter } from "../web/src/lib/types"

const unity: Compose = {
  parameters: [
    {
      name: "UNITY_EMAIL",
      label: "Unity email",
      description: "The Unity ID you sign in with.",
      type: "text",
      required: true,
      secret: false,
      default: "",
    },
    {
      name: "UNITY_PASSWORD",
      label: "",
      description: "",
      type: "text",
      required: true,
      secret: true,
      default: "",
    },
    {
      name: "BRANCH",
      label: "Branch",
      description: "",
      type: "text",
      required: false,
      secret: false,
      default: "main",
    },
  ],
  script: "set -eu\n",
  root: false,
  timeout_seconds: 600,
}

const record = (id: string, compose?: Compose) => ({
  image_id: `sha256:${id}`,
  reference: `bilskirnir/snapshot:${id}`,
  compose: compose ?? null,
})

test("the compose comes from a record of the chosen image, as the manager picks it", () => {
  const records = [record("plain"), record("unity", unity), record("again", unity)]
  assert.equal(composeFor(records, "sha256:unity"), unity)
  assert.equal(composeFor(records, "bilskirnir/snapshot:again"), unity)
  // A catalog reference or an image without one asks for nothing.
  assert.equal(composeFor(records, "sha256:plain"), undefined)
  assert.equal(composeFor(records, "ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04"), undefined)
  assert.equal(composeFor(records, undefined), undefined)
  // Two records of one image: the first, newest one with a compose wins.
  const shared = [
    { ...record("same"), reference: "a" },
    { ...record("same", { ...unity, root: true }), reference: "b" },
  ]
  assert.equal(composeFor(shared, "sha256:same")?.root, true)
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

test("a compose survives the editor unchanged, and a secret loses its default", () => {
  assert.deepEqual(composeBody(composeDraft(unity)), unity)
  const draft = composeDraft(unity)
  draft.parameters[2] = { ...draft.parameters[2], secret: true, name: " TOKEN " }
  const body = composeBody(draft)
  assert.deepEqual(body.parameters[2], {
    name: "TOKEN",
    label: "Branch",
    description: "",
    type: "text",
    required: false,
    secret: true,
    default: "",
  })
  // A new compose starts with a script skeleton and the manager's defaults.
  const fresh = composeDraft()
  assert.deepEqual([fresh.parameters, fresh.root, fresh.timeout_seconds], [[], false, "600"])
  assert.match(fresh.script, /set -euo pipefail/)
})

test("a pasted compose fills in what it leaves out and says what is wrong", () => {
  assert.deepEqual(readCompose({ script: "true", parameters: [{ name: "A" }] }), {
    parameters: [
      {
        name: "A",
        label: "",
        description: "",
        type: "text",
        required: false,
        secret: false,
        default: "",
      },
    ],
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
    assert.throws(() => readCompose(value), message, JSON.stringify(value))
})

const flavors = [
  { value: "vanilla", label: "Vanilla" },
  { value: "chocolate", label: "" },
]

// Each type with the fields only it has, as the manager writes them back.
const typed: ComposeParameter[] = [
  {
    name: "NOTES",
    label: "",
    description: "",
    type: "text",
    display: "multiline",
    required: false,
    secret: false,
    default: "a\nb",
  },
  {
    name: "THREADS",
    label: "",
    description: "",
    type: "number",
    display: "slider",
    required: true,
    secret: false,
    default: "4",
    min: 1,
    max: 16,
    step: 1,
  },
  {
    name: "PORT",
    label: "",
    description: "",
    type: "number",
    required: false,
    secret: false,
    default: "",
    min: 1024,
  },
  {
    name: "ACCEPT",
    label: "",
    description: "",
    type: "boolean",
    display: "checkbox",
    required: true,
    secret: false,
    default: "",
  },
  {
    name: "FLAVOR",
    label: "",
    description: "",
    type: "choice",
    display: "radio",
    required: false,
    secret: false,
    default: "mint",
    options: flavors,
    allow_custom: true,
  },
  {
    name: "TOPPINGS",
    label: "",
    description: "",
    type: "choices",
    required: false,
    secret: false,
    default: "chocolate",
    options: flavors,
  },
]

test("every type survives the editor unchanged", () => {
  const compose = { ...unity, parameters: typed }
  assert.deepEqual(composeBody(composeDraft(compose)), compose)
})

test("a new type starts over without the fields the old one had", () => {
  const [, threads, , , flavor] = composeDraft({ ...unity, parameters: typed }).parameters
  const text = withType(threads, "text")
  assert.deepEqual([text.display, text.default], ["input", ""])
  // Bounds stay in the draft, but a text parameter does not send them.
  assert.equal("min" in draftParameter(text), false)
  const many = draftParameter(withType(flavor, "choices"))
  assert.deepEqual([many.display, many.options, many.allow_custom], [undefined, flavors, undefined])
  const secret = withType({ ...composeDraft(unity).parameters[1] }, "number")
  assert.equal(draftParameter(secret).secret, false)
  // A choice starts with one empty option to fill in.
  const choice = withType(text, "choice")
  assert.equal(choice.options.length, 1)
  assert.equal(choice.options[0].value, "")
})

test("removing an option drops a default that picked it", () => {
  const [, , , , flavor, toppings] = composeDraft({ ...unity, parameters: typed }).parameters
  const vanillaOnly = (draft: typeof flavor) => [draft.options[0]]
  assert.equal(withOptions(toppings, vanillaOnly(toppings)).default, "")
  assert.equal(
    withOptions({ ...toppings, default: "vanilla\nchocolate" }, vanillaOnly(toppings)).default,
    "vanilla"
  )
  // Another value is allowed anyway, so a custom default stays.
  assert.equal(withOptions(flavor, vanillaOnly(flavor)).default, "mint")
  const strict = { ...flavor, allow_custom: false, default: "chocolate" }
  assert.equal(withOptions(strict, vanillaOnly(strict)).default, "")
  assert.equal(withOptions(strict, [...strict.options, newOption()]).default, "chocolate")
})

test("form values are written the way the script reads them", () => {
  assert.equal(toggleChoice("", "chocolate", true, flavors), "chocolate")
  // Lines follow the options' order, whatever order they were picked in.
  assert.equal(toggleChoice("chocolate", "vanilla", true, flavors), "vanilla\nchocolate")
  assert.equal(toggleChoice("vanilla\nchocolate", "vanilla", false, flavors), "chocolate")
  assert.equal(sliderValue(0.30000000000000004, 0.1), "0.3")
  assert.equal(sliderValue(7, null), "7")
  assert.equal(sliderValue(2.5, 0.25), "2.5")
  // A display the type does not have falls back to the type's own.
  assert.equal(displayOf({ type: "boolean", display: "slider" }), "toggle")
  assert.equal(displayOf({ type: "choice" }), "select")
})

test("a pasted compose keeps typed fields and refuses shapes the manager would", () => {
  const pasted = readCompose({ script: "true", parameters: typed })
  assert.deepEqual(pasted.parameters, typed)
  const cases: [unknown, RegExp][] = [
    [{ name: "A", type: "select" }, /unknown type/],
    [{ name: "A", type: "text", display: "slider" }, /cannot be shown/],
    [{ name: "A", type: "choice", options: [{ label: "x" }] }, /parameter A/],
    [{ name: "A", type: "number", min: "1" }, /parameter A/],
    [{ name: "A", description: 5 }, /parameter A/],
  ]
  for (const [parameter, message] of cases)
    assert.throws(() => readCompose({ script: "true", parameters: [parameter] }), message)
})
