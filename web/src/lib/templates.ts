import type {
  ContainerSetup,
  ImageRecord,
  ParameterDisplay,
  ParameterOption,
  ParameterType,
  Template,
  TemplateParameter,
} from "./types"

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
}

export const TYPE_LABELS: Record<ParameterType, string> = {
  text: "Text",
  number: "Number",
  boolean: "On or off",
  choice: "One choice",
  choices: "Several choices",
}

export const DISPLAY_LABELS: Record<ParameterDisplay, string> = {
  input: "Field",
  multiline: "Text area",
  slider: "Slider",
  toggle: "Switch",
  checkbox: "Checkbox",
  select: "Menu",
  radio: "Radio buttons",
  checkboxes: "Checkboxes",
}

export const displayOf = (parameter: Pick<TemplateParameter, "type" | "display">) =>
  parameter.display && DISPLAYS[parameter.type].includes(parameter.display)
    ? parameter.display
    : DISPLAYS[parameter.type][0]

const hasOptions = (type: ParameterType) => type === "choice" || type === "choices"

type Templated = Pick<ImageRecord, "image_id" | "reference" | "template">

/**
 * The template a new container from `value`, an image ID or reference, asks for. The manager
 * takes it from the first record of that image that has one; lists come newest first.
 */
export function templateFor(records: Templated[], value?: string) {
  if (!value) return undefined
  return (
    records.find(
      (record) => record.template && (record.image_id === value || record.reference === value)
    )?.template ?? undefined
  )
}

/** The values a create request sends: one for every parameter, its default when left alone. */
export function parameterBody(
  template: Pick<Template, "parameters"> | null | undefined,
  values: Record<string, string>
) {
  return Object.fromEntries(
    (template?.parameters ?? []).map((parameter) => [
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
}
export type TemplateDraft = {
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
})

const bound = (value?: number | null) => (value === undefined || value === null ? "" : `${value}`)

function parameterDraft(parameter: TemplateParameter): ParameterDraft {
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
export function draftParameter(draft: ParameterDraft): TemplateParameter {
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
  }
}

export function templateDraft(template?: Template | null): TemplateDraft {
  return {
    parameters: (template?.parameters ?? []).map(parameterDraft),
    script: template?.script ?? DEFAULT_SETUP,
    root: template?.root ?? false,
    timeout_seconds: String(template?.timeout_seconds ?? 600),
  }
}

/** The template a draft describes. */
export function templateBody(draft: TemplateDraft): Template {
  return {
    parameters: draft.parameters.map(draftParameter),
    script: draft.script,
    root: draft.root,
    timeout_seconds: Number(draft.timeout_seconds),
  }
}

const TYPES = Object.keys(DISPLAYS) as ParameterType[]

function readParameter(value: unknown): TemplateParameter {
  const parameter = (value ?? {}) as Record<string, unknown>
  const { name } = parameter
  if (typeof name !== "string" || !name) throw new Error("Every parameter needs a name.")
  const wrong = () => new Error(`The parameter ${name} is not shaped like a template parameter.`)
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
  if (!TYPES.includes(type)) throw new Error(`The parameter ${name} has an unknown type.`)
  const display = parameter.display ?? undefined
  if (display !== undefined && !DISPLAYS[type].includes(display as ParameterDisplay))
    throw new Error(`The parameter ${name} cannot be shown that way.`)
  const options = parameter.options ?? []
  if (!Array.isArray(options)) throw wrong()
  const read: TemplateParameter = {
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
  }
  for (const field of ["min", "max", "step"] as const) {
    const bound = parameter[field]
    if (bound === undefined || bound === null) continue
    if (typeof bound !== "number") throw wrong()
    read[field] = bound
  }
  return read
}

/** A pasted configuration's template, or the reason it cannot be one. */
export function readTemplate(value: unknown): Template {
  if (typeof value !== "object" || value === null || Array.isArray(value))
    throw new Error("The template must be a JSON object.")
  const {
    parameters = [],
    script,
    root = false,
    timeout_seconds = 600,
  } = value as Record<string, unknown>
  if (typeof script !== "string" || !script.trim())
    throw new Error("The template has no setup script.")
  if (!Array.isArray(parameters)) throw new Error("The template's parameters must be a list.")
  if (typeof root !== "boolean")
    throw new Error("The template's root setting must be true or false.")
  if (typeof timeout_seconds !== "number")
    throw new Error("The template's timeout must be a number of seconds.")
  return { parameters: parameters.map(readParameter), script, root, timeout_seconds }
}

const setupCopy: Record<ContainerSetup["status"], string> = {
  pending: "Waits for the desktop",
  running: "Running",
  completed: "Completed",
  failed: "Failed",
}

export const setupLabel = (setup: Pick<ContainerSetup, "status">) => setupCopy[setup.status]
