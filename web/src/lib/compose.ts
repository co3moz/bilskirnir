import type {
  ContainerSetup,
  ImageItem,
  ParameterDisplay,
  ParameterOption,
  ParameterType,
  Compose,
  ComposeParameter,
  Volume,
} from "./types"
import { t } from "./i18n"

/** The manager's rule for a parameter name, as an input pattern. */
export const PARAMETER_NAME = "[A-Z][A-Z0-9_]{0,63}"

export const DEFAULT_SETUP = `#!/usr/bin/env bash
set -euo pipefail
# Each parameter is an environment variable here, such as "$UNITY_EMAIL".
`

/** How the create form can show each type, the default first; the manager accepts these. */
export const DISPLAYS: Record<ParameterType, ParameterDisplay[]> = {
  text: ["input", "multiline"],
  number: ["input", "slider"],
  boolean: ["toggle", "checkbox"],
  choice: ["select", "radio"],
  choices: ["checkboxes"],
  volume: ["select"],
}

export const typeLabel = (type: ParameterType) => t(`compose.type.${type}`)

export const displayLabel = (display: ParameterDisplay) => t(`compose.display.${display}`)

export const displayOf = (parameter: Pick<ComposeParameter, "type" | "display">) =>
  parameter.display && DISPLAYS[parameter.type].includes(parameter.display)
    ? parameter.display
    : DISPLAYS[parameter.type][0]

const hasOptions = (type: ParameterType) => type === "choice" || type === "choices"

type WithCompose = Pick<ImageItem, "image_id" | "reference" | "compose">

/**
 * The compose a new container from `value`, an image ID or reference, asks for. The manager
 * takes it from the first record of that image that has one; lists come newest first.
 */
export function composeFor(records: WithCompose[], value?: string) {
  if (!value) return undefined
  return (
    records.find(
      (record) => record.compose && (record.image_id === value || record.reference === value)
    )?.compose ?? undefined
  )
}

/** The values a create request sends: one for every parameter, its default when left alone. */
export function parameterBody(
  compose: Pick<Compose, "parameters"> | null | undefined,
  values: Record<string, string>
) {
  return Object.fromEntries(
    (compose?.parameters ?? []).map((parameter) => [
      parameter.name,
      values[parameter.name] ?? parameter.default,
    ])
  )
}

/** The options picked in a `choices` value, which holds one per line. */
export const pickedChoices = (value: string) => (value ? value.split("\n") : [])

/** A `choices` value with `option` turned on or off, its lines in the options' order. */
export function toggleChoice(
  value: string,
  option: string,
  on: boolean,
  options: ParameterOption[]
) {
  const picked = new Set(pickedChoices(value))
  if (on) picked.add(option)
  else picked.delete(option)
  return options
    .map((item) => item.value)
    .filter((item) => picked.has(item))
    .join("\n")
}

/** The manager's rule for a volume name, as an input pattern and a check. */
export const VOLUME_NAME = "[a-zA-Z0-9][a-zA-Z0-9_.\\-]+"
export const volumeNameValid = (name: string) =>
  name.length <= 128 && new RegExp(`^${VOLUME_NAME}$`).test(name)

/** The volumes a volume parameter offers: every named volume, and its value if that is gone. */
export function volumeChoices(volumes: Pick<Volume, "Name">[], value: string) {
  const names = volumes.map((volume) => volume.Name).sort((a, b) => a.localeCompare(b))
  return value && !names.includes(value) ? [value, ...names] : names
}

/** A name for a new volume of `parameter`, such as `unity-home` for UNITY_HOME, not yet taken. */
export function newVolumeName(
  parameter: Pick<ComposeParameter, "name">,
  volumes: Pick<Volume, "Name">[]
) {
  // Parameter names start with a letter, so the name does too.
  const base = parameter.name.toLowerCase().replace(/_+/g, "-").replace(/-+$/, "")
  const taken = new Set(volumes.map((volume) => volume.Name))
  let name = base.length < 2 ? `${base}-volume` : base
  for (let number = 2; taken.has(name); number++) name = `${base}-${number}`
  return name
}

/**
 * A text parameter's pattern as this browser checks it, matching a whole value as the manager
 * does, or `null` when the browser cannot read it. The manager reads Rust's syntax, which this
 * only approximates, so its answer is the one that counts either way.
 */
export function browserPattern(pattern: string) {
  try {
    // On its own first, so a pattern such as `a)|(b` cannot close the group around it.
    new RegExp(pattern, "u")
    return new RegExp(`^(?:${pattern})$`, "u")
  } catch {
    return null
  }
}

/**
 * Whether a text value has its pattern's format, or `undefined` when only the manager can tell.
 * An empty value is left to `required`.
 */
export const patternMatches = (pattern: string | undefined, value: string) =>
  !pattern || !value || browserPattern(pattern)?.test(value)

/** A slider position as the script reads it, with no more decimals than its step has. */
export function sliderValue(position: number, step?: number | null) {
  const decimals = String(step ?? 1).split(".")[1]?.length ?? 0
  return String(Number(position.toFixed(decimals)))
}

export type OptionDraft = ParameterOption & { key: number }
/** A parameter as the editor holds it: every field of every type, numbers as typed. */
export type ParameterDraft = {
  key: number
  name: string
  label: string
  description: string
  type: ParameterType
  display: ParameterDisplay
  required: boolean
  secret: boolean
  default: string
  options: OptionDraft[]
  allow_custom: boolean
  min: string
  max: string
  step: string
  path: string
  pattern: string
  pattern_message: string
}
export type ComposeDraft = {
  parameters: ParameterDraft[]
  script: string
  root: boolean
  timeout_seconds: string
}

let draftKey = 0

export const newOption = (): OptionDraft => ({ key: ++draftKey, value: "", label: "" })

export const newParameter = (): ParameterDraft => ({
  key: ++draftKey,
  name: "",
  label: "",
  description: "",
  type: "text",
  display: "input",
  required: false,
  secret: false,
  default: "",
  options: [],
  allow_custom: false,
  min: "",
  max: "",
  step: "",
  path: "",
  pattern: "",
  pattern_message: "",
})

const bound = (value?: number | null) => (value === undefined || value === null ? "" : `${value}`)

function parameterDraft(parameter: ComposeParameter): ParameterDraft {
  return {
    ...newParameter(),
    name: parameter.name,
    label: parameter.label,
    description: parameter.description,
    type: parameter.type,
    display: displayOf(parameter),
    required: parameter.required,
    secret: parameter.secret,
    default: parameter.default,
    options: (parameter.options ?? []).map((option) => ({ ...option, key: ++draftKey })),
    allow_custom: parameter.allow_custom ?? false,
    min: bound(parameter.min),
    max: bound(parameter.max),
    step: bound(parameter.step),
    path: parameter.path ?? "",
    pattern: parameter.pattern ?? "",
    pattern_message: parameter.pattern_message ?? "",
  }
}

/**
 * A draft with another type. The default and the fields only the old type had are cleared, since
 * the new type would refuse them; options stay when both types have them.
 */
export function withType(draft: ParameterDraft, type: ParameterType): ParameterDraft {
  return {
    ...draft,
    type,
    display: DISPLAYS[type][0],
    default: "",
    secret: type === "text" && draft.secret,
    allow_custom: type === "choice" && draft.allow_custom,
    options: hasOptions(type) && !draft.options.length ? [newOption()] : draft.options,
  }
}

/** A draft with new options. A default that picks an option no longer offered is dropped. */
export function withOptions(draft: ParameterDraft, options: OptionDraft[]): ParameterDraft {
  const offered = new Set(options.map((option) => option.value.trim()))
  const kept =
    draft.type === "choices"
      ? pickedChoices(draft.default)
          .filter((line) => offered.has(line))
          .join("\n")
      : draft.allow_custom || offered.has(draft.default)
        ? draft.default
        : ""
  return { ...draft, options, default: kept }
}

/** The parameter a draft describes, with only the fields its type has. */
export function draftParameter(draft: ParameterDraft): ComposeParameter {
  const secret = draft.type === "text" && draft.secret
  const number = (text: string) => (text.trim() ? { value: Number(text) } : undefined)
  const range = Object.fromEntries(
    (["min", "max", "step"] as const).flatMap((field) => {
      const value = number(draft[field])
      return draft.type === "number" && value ? [[field, value.value]] : []
    })
  )
  return {
    name: draft.name.trim(),
    label: draft.label.trim(),
    description: draft.description.trim(),
    type: draft.type,
    ...(draft.display !== DISPLAYS[draft.type][0] ? { display: draft.display } : {}),
    required: draft.required,
    secret,
    // Image lists show defaults, so a secret has none.
    default: secret ? "" : draft.default,
    ...(hasOptions(draft.type)
      ? {
          options: draft.options.map((option) => ({
            value: option.value.trim(),
            label: option.label.trim(),
          })),
        }
      : {}),
    ...(draft.type === "choice" && draft.allow_custom ? { allow_custom: true } : {}),
    ...range,
    ...(draft.type === "volume" ? { path: draft.path.trim() } : {}),
    // Spaces can be part of a pattern, and a message without one is never shown.
    ...(draft.type === "text" && draft.pattern
      ? {
          pattern: draft.pattern,
          ...(draft.pattern_message.trim()
            ? { pattern_message: draft.pattern_message.trim() }
            : {}),
        }
      : {}),
  }
}

export function composeDraft(compose?: Compose | null): ComposeDraft {
  return {
    parameters: (compose?.parameters ?? []).map(parameterDraft),
    script: compose?.script ?? DEFAULT_SETUP,
    root: compose?.root ?? false,
    timeout_seconds: String(compose?.timeout_seconds ?? 600),
  }
}

/** The compose a draft describes. */
export function composeBody(draft: ComposeDraft): Compose {
  return {
    parameters: draft.parameters.map(draftParameter),
    script: draft.script,
    root: draft.root,
    timeout_seconds: Number(draft.timeout_seconds),
  }
}

const TYPES = Object.keys(DISPLAYS) as ParameterType[]

function readParameter(value: unknown): ComposeParameter {
  const parameter = (value ?? {}) as Record<string, unknown>
  const { name } = parameter
  if (typeof name !== "string" || !name) throw new Error(t("compose.read.noName"))
  const wrong = () => new Error(t("compose.read.shape", { name }))
  const text = (key: string) => {
    const field = parameter[key] ?? ""
    if (typeof field !== "string") throw wrong()
    return field
  }
  const flag = (key: string) => {
    const field = parameter[key] ?? false
    if (typeof field !== "boolean") throw wrong()
    return field
  }
  const type = (parameter.type ?? "text") as ParameterType
  if (!TYPES.includes(type)) throw new Error(t("compose.read.type", { name }))
  const display = parameter.display ?? undefined
  if (display !== undefined && !DISPLAYS[type].includes(display as ParameterDisplay))
    throw new Error(t("compose.read.display", { name }))
  const options = parameter.options ?? []
  if (!Array.isArray(options)) throw wrong()
  const read: ComposeParameter = {
    name,
    label: text("label"),
    description: text("description"),
    type,
    ...(display ? { display: display as ParameterDisplay } : {}),
    required: flag("required"),
    secret: flag("secret"),
    default: text("default"),
    ...(options.length
      ? {
          options: options.map((option) => {
            const { value, label = "" } = (option ?? {}) as Record<string, unknown>
            if (typeof value !== "string" || typeof label !== "string") throw wrong()
            return { value, label }
          }),
        }
      : {}),
    ...(flag("allow_custom") ? { allow_custom: true } : {}),
    ...(text("path") ? { path: text("path") } : {}),
    ...(text("pattern") ? { pattern: text("pattern") } : {}),
    ...(text("pattern_message") ? { pattern_message: text("pattern_message") } : {}),
  }
  for (const field of ["min", "max", "step"] as const) {
    const bound = parameter[field]
    if (bound === undefined || bound === null) continue
    if (typeof bound !== "number") throw wrong()
    read[field] = bound
  }
  return read
}

/** A pasted configuration's compose, or the reason it cannot be one. */
export function readCompose(value: unknown): Compose {
  if (typeof value !== "object" || value === null || Array.isArray(value))
    throw new Error(t("compose.read.object"))
  const {
    parameters = [],
    script,
    root = false,
    timeout_seconds = 600,
  } = value as Record<string, unknown>
  if (typeof script !== "string" || !script.trim()) throw new Error(t("compose.read.script"))
  if (!Array.isArray(parameters)) throw new Error(t("compose.read.parameters"))
  if (typeof root !== "boolean") throw new Error(t("compose.read.root"))
  if (typeof timeout_seconds !== "number") throw new Error(t("compose.read.timeout"))
  return { parameters: parameters.map(readParameter), script, root, timeout_seconds }
}

export const setupLabel = (setup: Pick<ContainerSetup, "status">) =>
  t(`compose.setup.${setup.status}`)
