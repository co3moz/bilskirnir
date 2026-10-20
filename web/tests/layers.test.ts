import { test } from "bun:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"

import { compressedCopies, imageName, stepGroups } from "../src/lib/layers"
import type { ImageStep } from "../src/lib/types"

const step = (overrides: Partial<ImageStep>): ImageStep => ({
  instruction: "RUN true",
  kind: "run",
  size: 0,
  created: 1,
  layer: false,
  from: null,
  shared_with: [],
  ...overrides,
})

// A snapshot of a script build, newest first as the manager lists the steps.
const steps: ImageStep[] = [
  step({ kind: "commit", size: 5000, layer: true, shared_with: ["child"] }),
  step({ kind: "config", instruction: "LABEL bilskirnir.managed=true", from: "build" }),
  step({ size: 2000, layer: true, from: "build", shared_with: ["build", "child"] }),
  step({ kind: "config", instruction: "USER root", from: "build" }),
  step({ kind: "config", instruction: "CMD bash", from: "ubuntu-xfce", shared_with: ["build"] }),
  step({
    kind: "add",
    size: 80,
    layer: true,
    from: "ubuntu-xfce",
    shared_with: ["ubuntu-xfce", "build", "other"],
  }),
]

test("steps group by the image they came from and settings fold away", () => {
  const groups = stepGroups(steps, false)
  assert.deepEqual(
    groups.map((group) => [group.from, group.size, group.steps.length, group.hidden]),
    [
      [null, 5000, 1, 0],
      ["build", 2000, 1, 2],
      ["ubuntu-xfce", 80, 1, 1],
    ]
  )
  // The image a group came from is its heading, so only the others are named as sharing it.
  assert.deepEqual(groups[0].sharedWith, ["child"])
  assert.deepEqual(groups[1].sharedWith, ["child"])
  assert.deepEqual(groups[2].sharedWith, ["build", "other"])
  const all = stepGroups(steps, true)
  assert.deepEqual(
    all.map((group) => group.steps.length),
    [1, 3, 2]
  )
  // A group of settings alone has nothing to show until settings are asked for.
  const settingsOnly = [step({ kind: "config", from: "base" }), step({ size: 1, layer: true })]
  assert.equal(stepGroups(settingsOnly, false).length, 1)
  assert.equal(stepGroups(settingsOnly, true).length, 2)
  // An empty commit still wrote a layer, and keeps its row.
  assert.equal(stepGroups([step({ kind: "commit", layer: true })], false)[0].steps.length, 1)
})

test("compressed copies are what Docker counts beyond the unpacked layers", () => {
  assert.equal(compressedCopies({ size: 2630, layers_size: 1977 }), 653)
  assert.equal(compressedCopies({ size: 100, layers_size: 100 }), 0)
  assert.equal(compressedCopies({ size: 90, layers_size: 100 }), 0)
  assert.equal(imageName([{ id: "a", name: "Alpha" }], "a"), "Alpha")
  assert.equal(imageName([], "gone"), "gone")
})

test("the layers route the image drawer reads is served", () => {
  const routes = readFileSync(new URL("../../src/main.rs", import.meta.url), "utf8")
  assert.ok(
    routes.includes('.route("/api/images/{id}/layers",'),
    "src/main.rs must serve /api/images/{id}/layers"
  )
})
