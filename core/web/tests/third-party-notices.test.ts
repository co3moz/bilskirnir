import { expect, test } from "bun:test"
import { readFileSync } from "node:fs"

const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8")

// The components THIRD_PARTY_NOTICES.md names with a release, and the package that pins it.
const NAMED = {
  noVNC: "@novnc/novnc",
  "Remix Icon": "@remixicon/react",
  "Monaco Editor": "monaco-editor",
}

test("the notices name the release of each interface package they describe", () => {
  const notices = read("../../../THIRD_PARTY_NOTICES.md")
  const manifest = JSON.parse(read("../package.json"))
  const pinned: Record<string, string> = { ...manifest.dependencies, ...manifest.devDependencies }
  for (const [component, name] of Object.entries(NAMED)) {
    const row = notices.match(new RegExp(String.raw`^\| \[${component}\]\([^)]+\) (\S+) \|`, "m"))
    expect(row, `THIRD_PARTY_NOTICES.md has no row for ${component}`).not.toBeNull()
    expect(pinned[name], `package.json pins no ${name}`).toBeDefined()
    expect(row![1], `THIRD_PARTY_NOTICES.md names another ${component} than package.json`).toBe(
      pinned[name]
    )
  }
})
