import { test } from "bun:test"
import assert from "node:assert/strict"
import { readFileSync } from "node:fs"

import { CONCEPTS } from "../src/lib/concepts"

const css = readFileSync(new URL("../src/index.css", import.meta.url), "utf8")

/** The variables a block of index.css sets, such as `:root` or `.dark`. */
function block(selector: string) {
  const start = css.indexOf(`\n${selector} {`)
  assert.ok(start >= 0, `index.css has a ${selector} block`)
  const body = css.slice(start, css.indexOf("\n}", start))
  return new Map([...body.matchAll(/^\s*(--[\w-]+):\s*([^;]+);/gm)].map((m) => [m[1], m[2]]))
}

const themes = { light: block(":root"), dark: block(".dark") }

type Lab = [number, number, number]
const linear = (channel: number) =>
  channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4

/** A color's relative luminance and OKLab coordinates, from `#rrggbb` or `oklch(L C h)`. */
function parse(value: string): { luminance: number; lab: Lab } {
  const hex = /^#([0-9a-f]{6})$/i.exec(value)
  if (hex) {
    const [r, g, b] = [0, 2, 4].map((at) => linear(parseInt(hex[1].slice(at, at + 2), 16) / 255))
    const [l, m, s] = [
      0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b,
      0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b,
      0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b,
    ].map(Math.cbrt)
    return {
      luminance: 0.2126 * r + 0.7152 * g + 0.0722 * b,
      lab: [
        0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
      ],
    }
  }
  const oklch = /^oklch\(([\d.]+) ([\d.]+) ([\d.]+)/.exec(value)
  assert.ok(oklch, `${value} is a hex or an oklch color`)
  const [lightness, chroma, hue] = oklch.slice(1).map(Number)
  const angle = (hue * Math.PI) / 180
  // Only a grey's luminance is needed: OKLab's lightness cubed.
  return {
    luminance: chroma === 0 ? lightness ** 3 : NaN,
    lab: [lightness, chroma * Math.cos(angle), chroma * Math.sin(angle)],
  }
}

const contrast = (a: number, b: number) => (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05)
const distance = (a: Lab, b: Lab) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2])

test("every concept has a color in both themes and a Tailwind utility", () => {
  const utilities = block("@theme inline")
  for (const concept of CONCEPTS) {
    const name = `--concept-${concept}`
    assert.equal(utilities.get(`--color-concept-${concept}`), `var(${name})`, name)
    for (const [theme, variables] of Object.entries(themes)) {
      // A hex keeps the checks below exact; a value made of other variables would escape them.
      assert.match(variables.get(name) ?? "", /^#[0-9a-f]{6}$/i, `${name} in the ${theme} theme`)
    }
  }
})

test("concept colors and menu tones read on every surface they sit on", () => {
  const colors = [
    ...CONCEPTS.map((concept) => `concept-${concept}`),
    ...["info", "success", "warning", "transfer", "edit"],
  ]
  for (const [theme, variables] of Object.entries(themes)) {
    // Cards and pages, menus and selects, and a hovered or active row in a list or the sidebar.
    const surfaces = ["--card", "--glass-raised", "--accent", "--sidebar-accent"].map((name) => {
      const { luminance } = parse(variables.get(name)!)
      assert.ok(!Number.isNaN(luminance), `${name} is a grey`)
      return [name, luminance] as const
    })
    for (const name of colors) {
      const color = parse(variables.get(`--${name}`)!)
      for (const [surface, luminance] of surfaces) {
        const ratio = contrast(color.luminance, luminance)
        assert.ok(
          ratio >= 4.5,
          `--${name} on ${surface} in the ${theme} theme is ${ratio.toFixed(2)}:1, below 4.5:1`
        )
      }
    }
  }
})

test("concepts are told apart from each other and from the state colors", () => {
  // An OKLab distance: 0.02 is barely visible side by side, a small icon needs far more.
  const apart = 0.08
  for (const [theme, variables] of Object.entries(themes)) {
    const color = (name: string) => parse(variables.get(name)!).lab
    const states = ["--success", "--warning", "--destructive"]
    CONCEPTS.forEach((concept, index) => {
      const name = `--concept-${concept}`
      const others = [...CONCEPTS.slice(index + 1).map((other) => `--concept-${other}`), ...states]
      for (const other of others) {
        const gap = distance(color(name), color(other))
        assert.ok(
          gap >= apart,
          `${name} and ${other} in the ${theme} theme are ${gap.toFixed(3)} apart, under ${apart}`
        )
      }
    })
  }
})
