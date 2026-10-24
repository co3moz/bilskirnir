import { expect, test } from "bun:test"

import { gplCompatible } from "../src/lib/licenses"

test("a license expression is compatible when GPL-3.0-or-later can carry the terms it leaves", () => {
  expect(gplCompatible("MIT")).toBe(true)
  expect(gplCompatible("Apache-2.0")).toBe(true)
  expect(gplCompatible("(MPL-2.0 OR Apache-2.0)")).toBe(true)
  expect(gplCompatible("MIT AND ISC")).toBe(true)
  expect(gplCompatible("(MIT OR Apache-2.0) AND Unicode-3.0")).toBe(false)
  expect(gplCompatible("Remix Icon License 1.0")).toBe(false)
  expect(gplCompatible("SEE LICENSE IN LICENSE.md")).toBe(false)
  expect(gplCompatible("CC-BY-4.0 OR MIT")).toBe(true)
  expect(gplCompatible("CC-BY-4.0 AND MIT")).toBe(false)
  expect(gplCompatible("Apache-2.0 WITH LLVM-exception")).toBe(true)
  expect(gplCompatible("(MIT")).toBe(false)
  expect(gplCompatible(undefined)).toBe(false)
})
