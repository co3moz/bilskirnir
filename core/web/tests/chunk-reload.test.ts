import { test } from "bun:test"
import assert from "node:assert/strict"

import { isChunkError, mayReload, RELOAD_GUARD } from "../src/lib/chunk-reload"

test("chunks a browser could not load are told from other errors", () => {
  for (const message of [
    // Chrome, Firefox and Safari word a failed dynamic import differently; Vite a stylesheet.
    "Failed to fetch dynamically imported module: http://thoric/assets/desktop-abc.js",
    "error loading dynamically imported module: http://thoric/assets/desktop-abc.js",
    "Importing a module script failed.",
    "Unable to preload CSS for /assets/usage-charts-abc.css",
  ])
    assert.equal(isChunkError(new Error(message)), true, message)
  assert.equal(isChunkError(new TypeError("Cannot read properties of undefined")), false)
  assert.equal(isChunkError(undefined), false)
})

test("a failed chunk reloads once, and again only after the guard has passed", () => {
  const stored = new Map<string, string>()
  const store = {
    getItem: (key: string) => stored.get(key) ?? null,
    setItem: (key: string, value: string) => void stored.set(key, value),
  }
  assert.equal(mayReload(store, 1000), true)
  // The reloaded page failing again at once shows its error instead of reloading in a loop.
  assert.equal(mayReload(store, 1000 + RELOAD_GUARD - 1), false)
  assert.equal(mayReload(store, 1000 + RELOAD_GUARD), true)
})

test("without a working store the page never reloads by itself", () => {
  assert.equal(mayReload(undefined), false)
  const refusing = {
    getItem: () => null,
    setItem: () => {
      throw new Error("QuotaExceededError")
    },
  }
  assert.equal(mayReload(refusing), false)
})
