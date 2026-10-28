// What the tag input adds and offers, apart from how it looks.

/** Whether `text` matches an input's `pattern` the way browsers read it, whole and with the v flag. */
export function matchesPattern(text: string, pattern?: string) {
  return !pattern || new RegExp(`^(?:${pattern})$`, "v").test(text)
}

/** Pasted or typed text cut at its commas and spaces: the names before the last cut, and the rest. */
export function splitTags(text: string) {
  const parts = text.split(/[\s,]+/)
  const rest = parts.pop() ?? ""
  return { names: parts.filter(Boolean), rest }
}

export type TagOption = { name: string; isNew: boolean }

/**
 * The list under a tag input: what was typed, when it is a valid name nothing offers yet, then the
 * suggestions not chosen that contain it, the one it names exactly first.
 */
export function tagOptions(
  typed: string,
  chosen: readonly string[],
  suggestions: readonly string[],
  valid: boolean
): TagOption[] {
  const matches = suggestions
    .filter((name) => !chosen.includes(name) && name.includes(typed))
    .sort((a, b) => Number(b === typed) - Number(a === typed))
  const isNew = valid && !chosen.includes(typed) && !suggestions.includes(typed)
  return [
    ...(isNew ? [{ name: typed, isNew }] : []),
    ...matches.map((name) => ({ name, isNew: false })),
  ]
}
