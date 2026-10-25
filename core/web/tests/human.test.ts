import { describe, expect, test } from "bun:test"
import { readFileSync } from "node:fs"

import { answerBody, askedAgo, newRequests, requestsFor, withRequest } from "../src/lib/human"
import type { HumanRequest } from "../src/lib/types"

const request = (id: string, extra: Partial<HumanRequest> = {}): HumanRequest => ({
  id,
  container_id: "c1",
  container_name: "Chrome dev",
  message: "Solve the CAPTCHA",
  endpoint: null,
  status: "pending",
  reply: null,
  created_at: 100,
  answered_at: null,
  ...extra,
})

describe("requests for a person", () => {
  test("a new request joins the waiting ones in the order they came, an answered one leaves", () => {
    const waiting = [request("a", { created_at: 100 }), request("c", { created_at: 300 })]
    const joined = withRequest(waiting, request("b", { created_at: 200 }))
    expect(joined?.map((item) => item.id)).toEqual(["a", "b", "c"])
    // Heard twice, by its event and by a read of the list, it is listed once.
    expect(withRequest(joined, request("b", { created_at: 200 }))).toBe(joined)
    const answered = withRequest(joined, request("b", { status: "done", answered_at: 400 }))
    expect(answered?.map((item) => item.id)).toEqual(["a", "c"])
    expect(withRequest(waiting, request("x", { status: "declined" }))).toBe(waiting)
    // Before the list was read there is nothing to change; the read brings it.
    expect(withRequest(undefined, request("a"))).toBeUndefined()
  })

  test("finds what a later read brought and what one desktop waits for", () => {
    const before = [request("a")]
    const after = [request("a"), request("b", { container_id: "c2" })]
    expect(newRequests(before, after).map((item) => item.id)).toEqual(["b"])
    expect(requestsFor(after, "c2").map((item) => item.id)).toEqual(["b"])
  })

  test("says how long ago a request came to the minute", () => {
    expect(askedAgo(1000, 1030)).toBe("just now")
    // A clock a little behind the manager's is not in the future.
    expect(askedAgo(1000, 990)).toBe("just now")
    expect(askedAgo(1000, 1000 + 5 * 60 + 59)).toBe("5 min ago")
    expect(askedAgo(1000, 1000 + 2 * 3600 + 10)).toBe("2 h ago")
  })

  test("sends a reply only when there is one", () => {
    expect(answerBody("done", "  ")).toEqual({ answer: "done" })
    expect(answerBody("decline", " No phone at hand ")).toEqual({
      answer: "decline",
      reply: "No phone at hand",
    })
  })
})

test("the routes the page uses are served and documented", () => {
  const routes = readFileSync(new URL("../../manager/src/main.rs", import.meta.url), "utf8")
  const guide = readFileSync(new URL("../../../docs/API.md", import.meta.url), "utf8")
  for (const route of ["/api/human-requests", "/api/human-requests/{id}/answer"]) {
    expect(routes).toContain(`"${route}"`)
    expect(guide).toContain(`\`${route}\``)
  }
})
