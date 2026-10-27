// A template's options: choices made when it is installed, such as leaving out an optional tool.
// They are resolved here, in the browser: each value is written at the top of its template's
// script as `NAME=true` or `NAME=false`, and the desktop MCP servers of an option that is off are
// left out. The manager never sees options, so the image's record keeps the script that ran, and
// a template's script that sets its own defaults with `: "${NAME:=true}"` builds the same way
// from a release, or an agent, that sends it as it is.
import type { BuildPart, DraftPart, ServerDeclaration, TemplateOption } from "./types"

/** The values chosen in the form, by `optionKey`; an option without one has its default. */
export type OptionValues = Record<string, boolean>

/** Where an option's value is kept: by its name, or `<part>.<NAME>` for a merged part's. */
export const optionKey = (name: string, part?: string) => (part ? `${part}.${name}` : name)

/** Whether an option is on: as chosen, or by its default. */
export const optionOn = (option: TemplateOption, values: OptionValues, part?: string) =>
  values[optionKey(option.name, part)] ?? option.default

/** The line before the values, which says where they came from to a reader of the script. */
export const OPTIONS_COMMENT = "# Template options chosen at install"

/**
 * The script with the options' values written at its top, after a leading `#!` line so it still
 * names its interpreter. A script without options is returned as it is.
 */
export function withOptions(
  script: string,
  options: TemplateOption[] | undefined,
  values: OptionValues,
  part?: string
) {
  if (!options?.length) return script
  const lines = [
    OPTIONS_COMMENT,
    ...options.map((option) => `${option.name}=${optionOn(option, values, part)}`),
  ].join("\n")
  const shebang = /^#![^\n]*(\n|$)/.exec(script)?.[0]
  if (!shebang) return `${lines}\n${script}`
  const rest = script.slice(shebang.length)
  return `${shebang.endsWith("\n") ? shebang : `${shebang}\n`}${lines}\n${rest}`
}

/** One script's options as the form shows them: the build's own, or those of a merged part. */
export type OptionGroup = {
  options: TemplateOption[]
  /** The part's id and template name; neither for a build of one script. */
  part?: string
  name?: string
}

/** The groups that have options, in the order their scripts run. */
export function optionGroups(
  options: TemplateOption[] | undefined,
  parts: DraftPart[] | null | undefined
): OptionGroup[] {
  if (!parts) return options?.length ? [{ options }] : []
  return parts
    .filter((part) => part.options?.length)
    .map((part) => ({ options: part.options!, part: part.id, name: part.name }))
}

/** The names of the desktop MCP servers an option that is off leaves out of the image. */
export function serversLeftOut(groups: OptionGroup[], values: OptionValues) {
  return new Set(
    groups.flatMap(({ options, part }) =>
      options
        .filter((option) => !optionOn(option, values, part))
        .flatMap((option) => option.mcp ?? [])
    )
  )
}

/** The servers the image gets with these values. */
export function serversKept(
  servers: ServerDeclaration[],
  groups: OptionGroup[],
  values: OptionValues
) {
  const out = serversLeftOut(groups, values)
  return servers.filter((server) => !out.has(server.name))
}

/**
 * What the build request carries: the script, or every part, with its values written in, and the
 * servers that stay. Parts lose their options, which the manager would refuse.
 */
export function resolveOptions(
  build: {
    script: string
    parts: DraftPart[] | null
    options?: TemplateOption[]
    servers: ServerDeclaration[]
  },
  values: OptionValues
): { script?: string; parts?: BuildPart[]; mcp: ServerDeclaration[] } {
  const mcp = serversKept(build.servers, optionGroups(build.options, build.parts), values)
  if (build.parts)
    return {
      parts: build.parts.map(({ id, name, script, options }) => ({
        id,
        name,
        script: withOptions(script, options, values, id),
      })),
      mcp,
    }
  return { script: withOptions(build.script, build.options, values), mcp }
}
