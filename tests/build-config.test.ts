import { test } from "bun:test"
import assert from "node:assert/strict"

import { formatBuildConfig, parseBuildConfig } from "../web/src/lib/build-config"

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
  assert.deepEqual(parseBuildConfig('{"script":"echo hi","description":"from a newer release"}'), {
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
  ]
  for (const [text, message] of cases) assert.throws(() => parseBuildConfig(text), message, text)
})

test("a name of 100 characters outside the basic plane still fits", () => {
  const name = "🖥".repeat(100)
  assert.equal(parseBuildConfig(JSON.stringify({ script: "echo", name })).name, name)
})
