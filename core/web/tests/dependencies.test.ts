import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"

import {
  groupFilters,
  keeps,
  licenseFilter,
  listFileName,
  matches,
  type Dependency,
} from "../src/lib/dependencies"

const crate = (name: string, extra: Partial<Dependency> = {}): Dependency => ({
  group: "backend",
  name,
  version: "1.0.0",
  scope: "runtime",
  ...extra,
})

const crates = [
  crate("axum", { direct: true, license: "MIT", description: "Web framework" }),
  crate("tokio", { direct: true, license: "MIT" }),
  crate("syn", { scope: "build", license: "MIT OR Apache-2.0" }),
  crate("ring", { license: "Apache-2.0 AND ISC" }),
  crate("mystery"),
]

describe("dependency filters", () => {
  test("each filter counts what it keeps and empty ones are left out", () => {
    const { filters } = groupFilters(crates)
    expect(filters).toEqual([
      { value: "all", count: 5 },
      { value: "direct", count: 2 },
      { value: "runtime", count: 4 },
      { value: "build", count: 1 },
    ])
  })

  test("licenses come most used first, and a license keeps only its exact expression", () => {
    const { licenses } = groupFilters(crates)
    expect(licenses.map(({ license, count }) => [license, count])).toEqual([
      ["MIT", 2],
      ["Apache-2.0 AND ISC", 1],
      ["MIT OR Apache-2.0", 1],
    ])
    const mit = crates.filter((item) => keeps(item, licenseFilter("MIT")))
    expect(mit.map((item) => item.name)).toEqual(["axum", "tokio"])
  })

  test("infrastructure filters by kind", () => {
    const items: Dependency[] = [
      {
        group: "infrastructure",
        name: "ubuntu",
        version: "24.04",
        kind: "image",
        used_in: ["desktop", "wsl"],
      },
      {
        group: "infrastructure",
        name: "curl",
        kind: "package",
        source: "ubuntu:24.04",
        used_in: ["wsl"],
      },
    ]
    expect(groupFilters(items).filters).toEqual([
      { value: "all", count: 2 },
      { value: "image", count: 1 },
      { value: "package", count: 1 },
    ])
    expect(items.filter((item) => keeps(item, "package")).map((item) => item.name)).toEqual([
      "curl",
    ])
  })
})

describe("dependency search", () => {
  test("every word must appear somewhere in the dependency", () => {
    expect(matches(crates[0], "")).toBe(true)
    expect(matches(crates[0], "AXUM web")).toBe(true)
    expect(matches(crates[0], "axum tokio")).toBe(false)
    expect(matches(crates[2], "apache")).toBe(true)
    const curl: Dependency = {
      group: "infrastructure",
      name: "curl",
      kind: "package",
      source: "debian:bookworm-slim",
      used_in: ["manager"],
    }
    expect(matches(curl, "bookworm manager")).toBe(true)
  })
})

test("the saved list is named after its release", () => {
  expect(listFileName({ version: "0.6.3" })).toBe("bilskirnir-0.6.3-dependencies.json")
  expect(listFileName({ version: "1.0/dev" })).toBe("bilskirnir-1.0_dev-dependencies.json")
})

test("the route the page uses is served and documented", () => {
  const routes = readFileSync(new URL("../../manager/src/main.rs", import.meta.url), "utf8")
  const guide = readFileSync(new URL("../../../docs/API.md", import.meta.url), "utf8")
  expect(routes).toContain('"/api/dependencies"')
  expect(guide).toContain("`/api/dependencies`")
})
