import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"

import {
  groupNames,
  groupPeers,
  joinRefusal,
  knownGroups,
  LABEL_PATTERN,
  nameLabel,
} from "../src/lib/groups"
import { matchesPattern } from "../src/lib/tags"
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

  test("a field takes the names groups.rs takes, as a browser reads its pattern", () => {
    for (const name of ["build", "team-2", "a", "0", "a".repeat(63)])
      expect(matchesPattern(name, LABEL_PATTERN)).toBe(true)
    for (const name of ["Build", "-build", "build-", "bu ild", "bu,ild", "", "a".repeat(64)])
      expect(matchesPattern(name, LABEL_PATTERN)).toBe(false)
  })

  test("the groups in use and the members of a container's groups", () => {
    const items = [
      item("a", spec("Web", ["build", "data"])),
      item("b", spec("Tests", ["build"])),
      item("c", spec("Db", ["store"])),
      item("d", spec("Old", undefined as unknown as string[])),
    ]
    expect(
      knownGroups(items).map(({ name, members }) => [name, members.map((member) => member.id)])
    ).toEqual([
      ["build", ["a", "b"]],
      ["data", ["a"]],
      ["store", ["c"]],
    ])
    expect(groupPeers(items[1]!, items).map((other) => other.id)).toEqual(["a"])
    expect(groupPeers(items[3]!, items)).toEqual([])
  })

  test("a container joins a group unless the manager would refuse it", () => {
    const items = [
      item("a", spec("Web", ["build"], { aliases: ["api"] })),
      item("b", spec("Api", [])),
      item("c", spec("Db", [])),
      item("d", spec("Busy", ["g1", "g2", "g3", "g4", "g5", "g6", "g7", "g8"])),
      item("e", spec("Db", [], { hostname: "thoric-db2" })),
    ]
    // Api's name is the alias a member is reached by already.
    expect(joinRefusal(items[1]!, "build", items, [])).toEqual({
      kind: "clash",
      name: "api",
      other: "Web",
    })
    expect(joinRefusal(items[2]!, "build", items, [])).toBeNull()
    expect(joinRefusal(items[3]!, "build", items, [])).toEqual({ kind: "full" })
    // Two containers chosen together are held to each other too.
    expect(joinRefusal(items[4]!, "build", items, ["c"])).toEqual({
      kind: "clash",
      name: "db",
      other: "Db",
    })
    expect(joinRefusal(items[4]!, "data", items, [])).toBeNull()
  })
})
