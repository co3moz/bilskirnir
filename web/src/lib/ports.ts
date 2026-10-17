// A container's proxied ports as the settings form shows and edits them: one line of numbers.

/** What the field accepts: digits, separated by commas or spaces. The manager checks the range. */
export const PORTS_PATTERN = "[0-9, ]*"

export function formatPorts(ports: readonly number[] = []) {
  return ports.join(", ")
}

/** The ports the field lists, in its order. */
export function parsePorts(text: string) {
  return text
    .split(/[\s,]+/)
    .filter(Boolean)
    .map(Number)
}
