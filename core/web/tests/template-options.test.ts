import { test } from "bun:test"
import assert from "node:assert/strict"

import {
  OPTIONS_COMMENT,
  optionGroups,
  resolveOptions,
  serversKept,
  withOptions,
} from "../src/lib/template-options"
import type { DraftPart, TemplateOption } from "../src/lib/types"

const radare2: TemplateOption = {
  name: "RADARE2",
  label: "radare2 and r2mcp",
  default: true,
  mcp: ["radare2"],
}
const wine: TemplateOption = { name: "WINE", label: "Wine", default: false }
const options = [radare2, wine]
const SCRIPT = '#!/usr/bin/env bash\nset -euo pipefail\n: "${RADARE2:=true}"\n'
const r2Server = { name: "radare2", command: "r2mcp" }
const ghidraServer = { name: "ghidra", command: "ghidra-mcp" }

test("the values go after the script's #! line, the defaults when nothing was chosen", () => {
  assert.equal(
    withOptions(SCRIPT, options, {}),
    `#!/usr/bin/env bash\n${OPTIONS_COMMENT}\nRADARE2=true\nWINE=false\nset -euo pipefail\n: "\${RADARE2:=true}"\n`
  )
  assert.deepEqual(
    withOptions(SCRIPT, options, { RADARE2: false, WINE: true }).split("\n").slice(0, 4),
    ["#!/usr/bin/env bash", OPTIONS_COMMENT, "RADARE2=false", "WINE=true"]
  )
})

test("a script without a #! line gets the values at its top, and one without options is kept", () => {
  assert.equal(
    withOptions("apt-get update\n", [wine], {}),
    `${OPTIONS_COMMENT}\nWINE=false\napt-get update\n`
  )
  // A script that is only its #! line still names its interpreter first.
  assert.equal(withOptions("#!/bin/sh", [wine], {}), `#!/bin/sh\n${OPTIONS_COMMENT}\nWINE=false\n`)
  assert.equal(withOptions(SCRIPT, [], { WINE: true }), SCRIPT)
  assert.equal(withOptions(SCRIPT, undefined, {}), SCRIPT)
})

test("an option that is off leaves its desktop MCP servers out, and only those", () => {
  const groups = optionGroups(options, null)
  const servers = [ghidraServer, r2Server]
  assert.deepEqual(serversKept(servers, groups, {}), servers)
  assert.deepEqual(serversKept(servers, groups, { RADARE2: false }), [ghidraServer])
  // Turning on an option that names no server changes nothing.
  assert.deepEqual(serversKept(servers, groups, { WINE: true }), servers)
})

test("a request carries the resolved script and the servers kept, never the options", () => {
  const build = resolveOptions(
    { script: SCRIPT, parts: null, options, servers: [ghidraServer, r2Server] },
    { RADARE2: false }
  )
  assert.deepEqual(Object.keys(build), ["script", "mcp"])
  assert.match(
    build.script!,
    /^#!\/usr\/bin\/env bash\n# Template options .*\nRADARE2=false\nWINE=false\nset/
  )
  assert.deepEqual(build.mcp, [ghidraServer])
})

test("each merged part gets its own values, and the parts lose their options", () => {
  const parts: DraftPart[] = [
    { id: "reverse", name: "Reverse engineering", script: SCRIPT, options },
    { id: "chrome", name: "Google Chrome", script: "install chrome" },
    {
      id: "other",
      name: "Other",
      script: "#!/bin/bash\necho",
      options: [{ ...wine, default: true }],
    },
  ]
  // Only parts with options are listed, in the order they run, each under its template's name.
  assert.deepEqual(
    optionGroups(undefined, parts).map((group) => [group.part, group.name, group.options.length]),
    [
      ["reverse", "Reverse engineering", 2],
      ["other", "Other", 1],
    ]
  )
  // The same name in two parts is two values, kept as `<part>.<NAME>`.
  const build = resolveOptions(
    { script: "", parts, servers: [r2Server] },
    { "reverse.WINE": true, "reverse.RADARE2": false, "other.WINE": false }
  )
  assert.equal("script" in build, false)
  assert.deepEqual(
    build.parts!.map((part) => Object.keys(part)),
    [
      ["id", "name", "script"],
      ["id", "name", "script"],
      ["id", "name", "script"],
    ]
  )
  assert.deepEqual(build.parts![0].script.split("\n").slice(2, 4), ["RADARE2=false", "WINE=true"])
  assert.equal(build.parts![1].script, "install chrome")
  assert.equal(build.parts![2].script, `#!/bin/bash\n${OPTIONS_COMMENT}\nWINE=false\necho`)
  assert.deepEqual(build.mcp, [])
  // A part left out takes its options along: its servers stay as declared.
  assert.deepEqual(
    resolveOptions(
      { script: "", parts: parts.slice(1), servers: [r2Server] },
      { "reverse.RADARE2": false }
    ).mcp,
    [r2Server]
  )
})
