import type { EnvironmentSetup, ImageRecord, Template, TemplateParameter } from "./types"

/** The manager's rule for a parameter name, as an input pattern. */
export const PARAMETER_NAME = "[A-Z][A-Z0-9_]{0,63}"

export const DEFAULT_SETUP = `#!/usr/bin/env bash
set -euo pipefail
# Each parameter is an environment variable here, such as "$UNITY_EMAIL".
`

type Templated = Pick<ImageRecord, "image_id" | "reference" | "template">

/**
 * The template a new environment from `value`, an image ID or reference, asks for. The manager
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

export type ParameterDraft = TemplateParameter & { key: number }
export type TemplateDraft = {
  parameters: ParameterDraft[]
  script: string
  root: boolean
  timeout_seconds: string
}

let parameterKey = 0

export const newParameter = (): ParameterDraft => ({
  key: ++parameterKey,
  name: "",
  label: "",
  required: false,
  secret: false,
  default: "",
})

export function templateDraft(template?: Template | null): TemplateDraft {
  return {
    parameters: (template?.parameters ?? []).map((parameter) => ({
      ...parameter,
      key: ++parameterKey,
    })),
    script: template?.script ?? DEFAULT_SETUP,
    root: template?.root ?? false,
    timeout_seconds: String(template?.timeout_seconds ?? 600),
  }
}

/** The template a draft describes. A secret has no default, since image lists show defaults. */
export function templateBody(draft: TemplateDraft): Template {
  return {
    parameters: draft.parameters.map((parameter) => ({
      name: parameter.name.trim(),
      label: parameter.label.trim(),
      required: parameter.required,
      secret: parameter.secret,
      default: parameter.secret ? "" : parameter.default,
    })),
    script: draft.script,
    root: draft.root,
    timeout_seconds: Number(draft.timeout_seconds),
  }
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
  return {
    parameters: parameters.map((parameter) => {
      const { name, label = "", required = false, secret = false } = parameter ?? {}
      const fallback = parameter?.default ?? ""
      if (typeof name !== "string" || !name) throw new Error("Every parameter needs a name.")
      if (
        typeof label !== "string" ||
        typeof fallback !== "string" ||
        typeof required !== "boolean" ||
        typeof secret !== "boolean"
      )
        throw new Error(`The parameter ${name} is not shaped like a template parameter.`)
      return { name, label, required, secret, default: fallback }
    }),
    script,
    root,
    timeout_seconds,
  }
}

const setupCopy: Record<EnvironmentSetup["status"], string> = {
  pending: "Waits for the desktop",
  running: "Running",
  completed: "Completed",
  failed: "Failed",
}

export const setupLabel = (setup: Pick<EnvironmentSetup, "status">) => setupCopy[setup.status]
