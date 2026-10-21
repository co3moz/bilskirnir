// What the inspector's Monaco JSON editor is given, kept apart from Monaco so it can be tested:
// the schema of a field, the models' addresses, the JSON language service's options and the
// theme made from the interface's colors.
import { omit, type JsonSchema } from "./schema-form"

/**
 * The schema an editor checks a nested field against: the field's own, with the root's
 * definitions and dialect so `$ref`s inside it resolve, and without the form's markers.
 */
export function editorSchema(schema: JsonSchema, root: JsonSchema): JsonSchema {
  return {
    ...(root.$schema ? { $schema: root.$schema } : {}),
    ...(root.$defs ? { $defs: root.$defs } : {}),
    ...(root.definitions ? { definitions: root.definitions } : {}),
    ...omit(schema, "nullable", "typeList"),
  }
}

/** The address of an editor's model, which its schema is matched to. */
export const modelUri = (id: string) => `inmemory://bilskirnir/inspector/${id}.json`

/**
 * The JSON language service's options for the editors open now, each model matched to its own
 * schema by its address. Schemas are never fetched: the page may not reach other hosts, and a
 * `$ref` to one is left unchecked rather than reported.
 */
export function diagnosticsOptions(schemas: ReadonlyMap<string, JsonSchema>) {
  return {
    validate: true,
    allowComments: false,
    enableSchemaRequest: false,
    schemaRequest: "ignore" as const,
    schemaValidation: "error" as const,
    comments: "error" as const,
    trailingCommas: "error" as const,
    schemas: [...schemas].map(([model, schema]) => ({
      uri: `${model}#schema`,
      fileMatch: [model],
      schema,
    })),
  }
}

/**
 * The interface's code colors for the tokens of Monaco's JSON mode, as `CodeTextarea` colors
 * the same text: keys, strings, numbers, literals, punctuation and comments.
 */
export const JSON_TOKENS: Record<string, string> = {
  "string.key.json": "--code-variable",
  "string.value.json": "--code-string",
  "number.json": "--code-number",
  "keyword.json": "--code-keyword",
  "delimiter.bracket.json": "--code-operator",
  "delimiter.array.json": "--code-operator",
  "delimiter.colon.json": "--code-operator",
  "delimiter.comma.json": "--code-operator",
  "comment.line.json": "--code-comment",
  "comment.block.json": "--code-comment",
}

/** A color as Monaco takes it, `RRGGBB` or `RRGGBBAA`, from the RGBA of a rendered color. */
export function hexColor([red, green, blue, alpha]: readonly number[], withHash = true) {
  const part = (value: number) => Math.round(value).toString(16).padStart(2, "0")
  const hex = `${part(red)}${part(green)}${part(blue)}${alpha < 255 ? part(alpha) : ""}`
  return withHash ? `#${hex}` : hex
}

/** Monaco's token rules from the code colors, read through `color` as `RRGGBB`. */
export const tokenRules = (color: (variable: string) => string) =>
  Object.entries(JSON_TOKENS).map(([token, variable]) => ({
    token,
    foreground: color(variable).replace(/^#/, ""),
  }))

/** How tall an editor is for content of `height` pixels: between its least and its most. */
export const editorHeight = (height: number, least: number, most: number) =>
  Math.round(Math.min(Math.max(height, least), most))
