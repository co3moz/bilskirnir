import { test } from "bun:test"
import assert from "node:assert/strict"

import { holdUnload } from "../src/lib/unload"

test("the browser asks before closing the tab while any transfer runs", () => {
  const listeners = new Set<EventListenerOrEventListenerObject>()
  const target = {
    addEventListener: (_type: string, listener: EventListenerOrEventListenerObject) =>
      void listeners.add(listener),
    removeEventListener: (_type: string, listener: EventListenerOrEventListenerObject) =>
      void listeners.delete(listener),
  } as Pick<Window, "addEventListener" | "removeEventListener">
  const upload = holdUnload(target)
  const restore = holdUnload(target)
  assert.equal(listeners.size, 1)

  const [warn] = listeners as Set<(event: BeforeUnloadEvent) => void>
  let prevented = false
  const event = { preventDefault: () => (prevented = true), returnValue: undefined }
  warn(event as unknown as BeforeUnloadEvent)
  assert.equal(prevented, true)

  upload()
  // Letting go twice, as a cancel and the end of the same upload may, counts once.
  upload()
  assert.equal(listeners.size, 1)
  restore()
  assert.equal(listeners.size, 0)
})

test("outside a browser a transfer holds nothing", () => {
  const release = holdUnload(undefined)
  release()
})
