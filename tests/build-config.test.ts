import { test } from "bun:test"
import assert from "node:assert/strict"

import { formatBuildConfig, parseBuildConfig } from "../web/src/lib/build-config"

const PNG = "data:image/png;base64,"

const config = {
  name: "Unity tools",
  base_image: "ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04",
  internet: false,
  script: "#!/usr/bin/env bash\nset -euo pipefail\napt-get install -y git\n",
}

test("an exported configuration imports back unchanged, in the form's field order", () => {
  // The manager may send its keys in any order; the copy always reads the same way.
  const text = formatBuildConfig({
    script: config.script,
    internet: config.internet,
    base_image: config.base_image,
    name: config.name,
  })
  assert.deepEqual(Object.keys(JSON.parse(text)), ["name", "base_image", "internet", "script"])
  assert.deepEqual(parseBuildConfig(text), config)
})

test("a configuration needs only its script, and unknown fields are ignored", () => {
  assert.deepEqual(parseBuildConfig('{"script":"echo hi","labels":"from a newer release"}'), {
    script: "echo hi",
  })
})

test("a configuration that cannot fill the form says why", () => {
  const cases: [string, RegExp][] = [
    ["not json", /not valid JSON/],
    ['["echo hi"]', /JSON object/],
    ["null", /JSON object/],
    ['{"name":"No script"}', /no setup script/],
    ['{"script":"  \\n"}', /no setup script/],
    ['{"script":"echo","name":7}', /name must be text/],
    [JSON.stringify({ script: "echo", name: "x".repeat(101) }), /at most 100/],
    ['{"script":"echo","base_image":""}', /image reference/],
    ['{"script":"echo","internet":"yes"}', /true or false/],
    [JSON.stringify({ script: "echo", description: "x".repeat(4097) }), /4096 bytes/],
    // Bytes, as the manager counts them, not characters.
    [JSON.stringify({ script: "echo", description: "ş".repeat(2049) }), /4096 bytes/],
    ['{"script":"echo","icon":"data:image/gif;base64,R0lGOD"}', /PNG data URL/],
    [JSON.stringify({ script: "echo", icon: `${PNG}${"A".repeat(410_000)}` }), /300 KiB/],
    ['{"script":"echo","template_id":"Unity"}', /lowercase/],
    ['{"script":"echo","template_id":"-unity"}', /lowercase/],
  ]
  for (const [text, message] of cases) assert.throws(() => parseBuildConfig(text), message, text)
})

test("a description, an icon and a template id travel with the configuration", () => {
  const icon = `${PNG}iVBORw0KGgo=`
  const full = { ...config, description: "Git and LFS", icon, template_id: "unity-tools" }
  const text = formatBuildConfig(full)
  // The icon, the longest line, comes last so the rest stays readable.
  assert.deepEqual(Object.keys(JSON.parse(text)), [
    "name",
    "description",
    "base_image",
    "internet",
    "script",
    "template_id",
    "icon",
  ])
  assert.deepEqual(parseBuildConfig(text), full)
})

test("a name of 100 characters outside the basic plane still fits", () => {
  const name = "🖥".repeat(100)
  assert.equal(parseBuildConfig(JSON.stringify({ script: "echo", name })).name, name)
})

test("a configuration carries the image's compose both ways", () => {
  const compose = {
    parameters: [
      {
        name: "UNITY_EMAIL",
        label: "Email",
        description: "",
        type: "text" as const,
        required: true,
        secret: false,
        default: "",
      },
    ],
    script: "set -eu\n",
    root: false,
    timeout_seconds: 600,
  }
  const text = formatBuildConfig({ ...config, compose })
  assert.deepEqual(Object.keys(JSON.parse(text)), [
    "name",
    "base_image",
    "internet",
    "script",
    "compose",
  ])
  assert.deepEqual(parseBuildConfig(text), { ...config, compose })
  // A configuration copied while composes were templates still brings it along.
  const older = text.replace('"compose":', '"template":')
  assert.deepEqual(parseBuildConfig(older), {
    ...config,
    compose,
  })
  // Without one, the copy stays the plain build request.
  assert.equal("compose" in JSON.parse(formatBuildConfig(config)), false)
  assert.throws(
    () => parseBuildConfig(JSON.stringify({ ...config, compose: { parameters: [] } })),
    /compose has no setup script/
  )
})
