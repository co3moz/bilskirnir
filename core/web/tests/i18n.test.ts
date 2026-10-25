import { test } from "bun:test"
import assert from "node:assert/strict"

import {
  catalogs,
  detectLocale,
  getLocale,
  interpolate,
  locales,
  plural,
  rich,
  setLocale,
  t,
} from "../src/lib/i18n"
import { en } from "../src/locales/en"

const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort()

test("every language translates every message, with the same placeholders", () => {
  const keys = Object.keys(en)
  for (const locale of locales) {
    const catalog = catalogs[locale]
    assert.deepEqual(Object.keys(catalog).sort(), [...keys].sort(), locale)
    for (const key of keys) {
      const text = catalog[key as keyof typeof en]
      assert.ok(text.trim(), `${locale} ${key} is empty`)
      // A value the code fills in must have a place in every language.
      assert.deepEqual(
        placeholders(text),
        placeholders(en[key as keyof typeof en]),
        `${locale} ${key}`
      )
    }
  }
})

test("plural forms come in pairs", () => {
  for (const key of Object.keys(en)) {
    if (key.endsWith(".one")) assert.ok(`${key.slice(0, -4)}.other` in en, key)
    if (key.endsWith(".other")) assert.ok(`${key.slice(0, -6)}.one` in en, key)
  }
})

test("no message uses a dash the project's copy avoids", () => {
  for (const locale of locales)
    for (const [key, text] of Object.entries(catalogs[locale]))
      assert.ok(!/[–—]/.test(text), `${locale} ${key}: ${text}`)
})

test("the browser's first language with a catalog is chosen, English otherwise", () => {
  assert.equal(detectLocale(["tr-TR", "en-US"]), "tr")
  assert.equal(detectLocale(["pt-BR", "de-AT"]), "de")
  assert.equal(detectLocale(["FR-ca"]), "fr")
  assert.equal(detectLocale(["es_419"]), "es")
  assert.equal(detectLocale(["pt-BR", "ja"]), "en")
  assert.equal(detectLocale([]), "en")
})

test("placeholders are filled in, and a missing value keeps its place", () => {
  assert.equal(interpolate("Delete {name}?", { name: "Unity" }), "Delete Unity?")
  assert.equal(interpolate("{a} of {b}", { a: 1 }), "1 of {b}")
  assert.equal(t("delete.titleOf", { name: "Unity" }), "Delete Unity?")
  assert.deepEqual(
    rich("files.title", { volume: "VOLUME" }).map(
      (node) => (node as { props: { children: unknown } }).props.children
    ),
    ["Files in ", "VOLUME"]
  )
})

test("a chosen language applies at once, and the browser's language returns without one", () => {
  assert.equal(getLocale(), "en")
  try {
    setLocale("tr")
    assert.equal(getLocale(), "tr")
    assert.equal(t("nav.containers"), catalogs.tr["nav.containers"])
    // Plural rules and numbers follow the language too.
    assert.equal(
      plural(1, { one: "table.rows.one", other: "table.rows.other" }),
      interpolate(catalogs.tr["table.rows.one"], { count: "1" })
    )
  } finally {
    setLocale(null)
  }
  assert.equal(getLocale(), "en")
  assert.equal(plural(2, { one: "table.rows.one", other: "table.rows.other" }), "2 rows")
})
