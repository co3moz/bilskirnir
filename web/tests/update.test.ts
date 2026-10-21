import { describe, expect, test } from "bun:test"

import { next, type Answer } from "../src/lib/update"

const status = (current: string, running: boolean): Answer => ({
  kind: "status",
  current,
  running,
})

describe("following an update", () => {
  test("keeps following while the manager is away or the update still runs in it", () => {
    expect(next({ kind: "away" }, "0.6.0", false)).toBe("follow")
    expect(next({ kind: "away" }, "0.6.0", true)).toBe("follow")
    expect(next(status("0.6.0", true), "0.6.0", false)).toBe("follow")
  })

  test("reloads into a new manager however it shows", () => {
    // Another release answers.
    expect(next(status("0.7.0", true), "0.6.0", false)).toBe("reload")
    // A manager that was away answers again, even with the same release, such as latest.
    expect(next(status("0.6.0", true), "0.6.0", true)).toBe("reload")
    // The new manager does not know the page's session.
    expect(next({ kind: "signed-out" }, "0.6.0", false)).toBe("reload")
  })

  test("stops when the update ended in the manager that started it", () => {
    expect(next(status("0.6.0", false), "0.6.0", false)).toBe("done")
  })
})
