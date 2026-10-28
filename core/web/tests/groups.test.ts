import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"

import {
  formatNames,
  groupNames,
  groupPeers,
  knownGroups,
  nameLabel,
  NAMES_PATTERN,
  parseNames,
} from "../src/lib/groups"
import type { ContainerItem, ContainerSpec } from "../src/lib/types"

const cases: [string, string | null][] = JSON.parse(
  readFileSync(new URL("./name-labels.json", import.meta.url), "utf8")
)

const spec = (name: string, groups: string[], extra: Partial<ContainerSpec> = {}) =>
  ({ name, groups, hostname: `thoric-${name.toLowerCase()}`, ...extra }) as ContainerSpec
const item = (id: string, value: ContainerSpec) => ({ id, spec: value }) as ContainerItem

describe("container groups", () => {
  test("a name becomes the DNS label the manager gives it", () => {
    // groups.rs reads the same cases.
    expect(cases.length).toBeGreaterThan(5)
    for (const [name, label] of cases) expect(nameLabel(name)).toBe(label)
  })

  test("a member is reached by its hostname, name and aliases once each", () => {
    expect(
      groupNames(spec("Web", ["build"], { hostname: "Thoric-Web", aliases: ["api", "web"] }))
    ).toEqual(["thoric-web", "web", "api"])
    // A container created before identities were kept has no hostname yet.
    expect(groupNames(spec("Web", [], { hostname: null }))).toEqual(["web"])
  })

  test("the fields take names separated by commas or spaces", () => {
    expect(parseNames(" build,  tests  build ,")).toEqual(["build", "tests"])
    expect(formatNames(["build", "tests"])).toBe("build, tests")
    const field = new RegExp(`^${NAMES_PATTERN}$`)
    expect(field.test("build, team-2")).toBe(true)
    expect(field.test("Build")).toBe(false)
  })

  test("the groups in use and the members of a container's groups", () => {
    const items = [
      item("a", spec("Web", ["build", "data"])),
      item("b", spec("Tests", ["build"])),
      item("c", spec("Db", ["store"])),
      item("d", spec("Old", undefined as unknown as string[])),
    ]
    expect(knownGroups(items)).toEqual(["build", "data", "store"])
    expect(groupPeers(items[1]!, items).map((other) => other.id)).toEqual(["a"])
    expect(groupPeers(items[3]!, items)).toEqual([])
  })
})
