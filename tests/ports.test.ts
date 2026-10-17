import { test } from "bun:test"
import assert from "node:assert/strict"

import { formatPorts, parsePorts, PORTS_PATTERN } from "../web/src/lib/ports"

test("the ports field round-trips the setting and tolerates any separator spacing", () => {
  assert.equal(formatPorts([8090, 6400]), "8090, 6400")
  assert.equal(formatPorts(), "")
  assert.deepEqual(parsePorts(formatPorts([8090, 6400])), [8090, 6400])
  assert.deepEqual(parsePorts(" 8090,6400  80 ,"), [8090, 6400, 80])
  // An empty field clears the setting.
  assert.deepEqual(parsePorts(""), [])
  assert.deepEqual(parsePorts(" , "), [])
})

test("the field refuses anything but numbers before the manager sees it", () => {
  // The browser anchors a pattern to the whole value.
  const accepts = (value: string) => new RegExp(`^(?:${PORTS_PATTERN})$`, "v").test(value)
  for (const value of ["", "8090", "8090, 6400", "1 2 3"]) assert.ok(accepts(value), value)
  for (const value of ["http", "80-90", "8090;6400", "-1"]) assert.ok(!accepts(value), value)
})
