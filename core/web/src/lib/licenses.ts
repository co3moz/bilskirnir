/**
 * The licenses Thoric's own GPL-3.0-or-later can carry, by SPDX identifier, for the packages
 * the interface bundle combines with it. A package under any other, such as Remix Icon's own
 * license from @remixicon/react 4.9.0 on, cannot ship in the bundle.
 */
export const GPL_COMPATIBLE = new Set([
  "0BSD",
  "Apache-2.0",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "BSL-1.0",
  "CC0-1.0",
  "ISC",
  "MIT",
  "MPL-2.0",
  "Unlicense",
  "Zlib",
])

/**
 * Whether an SPDX license expression, such as `(MIT OR Apache-2.0) AND ISC`, leaves terms the
 * bundle can use: one side of each OR, both sides of each AND. A license with an exception
 * (`WITH`) counts as its license. Anything that does not parse is refused.
 */
export function gplCompatible(expression: unknown) {
  if (typeof expression !== "string") return false
  const tokens = expression.match(/[()]|[^\s()]+/g) ?? []
  let at = 0
  const license = (): boolean => {
    if (tokens[at] === "(") {
      at++
      const allowed = either()
      if (tokens[at] !== ")") throw new Error("unbalanced")
      at++
      return allowed
    }
    const name = tokens[at++]
    if (tokens[at] === "WITH") at += 2
    return name !== undefined && GPL_COMPATIBLE.has(name)
  }
  const both = () => {
    let allowed = license()
    while (tokens[at] === "AND") {
      at++
      allowed = license() && allowed
    }
    return allowed
  }
  const either = () => {
    let allowed = both()
    while (tokens[at] === "OR") {
      at++
      allowed = both() || allowed
    }
    return allowed
  }
  try {
    return either() && at === tokens.length
  } catch {
    return false
  }
}
