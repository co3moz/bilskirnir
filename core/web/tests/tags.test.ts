import { describe, expect, test } from "bun:test"

import { splitTags, tagOptions } from "../src/lib/tags"

const names = (options: ReturnType<typeof tagOptions>) =>
  options.map(({ name, isNew }) => (isNew ? `+${name}` : name))

describe("tag input", () => {
  test("pasted text is cut at commas and spaces, the last part still being typed", () => {
    expect(splitTags("build, tests  data")).toEqual({ names: ["build", "tests"], rest: "data" })
    expect(splitTags("build,")).toEqual({ names: ["build"], rest: "" })
    expect(splitTags("bui")).toEqual({ names: [], rest: "bui" })
  })

  test("the list offers what was typed first, then the suggestions that contain it", () => {
    const suggestions = ["build", "data", "rebuild"]
    expect(names(tagOptions("bui", [], suggestions, true))).toEqual(["+bui", "build", "rebuild"])
    // A name the list offers already is not offered twice, and it comes first.
    expect(names(tagOptions("build", [], ["rebuild", "build"], true))).toEqual(["build", "rebuild"])
    // Chosen names leave the list; an invalid name is not offered as new.
    expect(names(tagOptions("", ["data"], suggestions, false))).toEqual(["build", "rebuild"])
    expect(names(tagOptions("bu-", [], suggestions, false))).toEqual([])
    expect(names(tagOptions("data", ["data"], suggestions, true))).toEqual([])
  })
})
