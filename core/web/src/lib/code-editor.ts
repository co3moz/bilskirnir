// What the Monaco code editor is given, kept apart from Monaco so it can be tested: the schema
// of a JSON field, the models' addresses and languages, the JSON language service's options and
// the theme made from the interface's colors.
import type { CodeLanguage } from "./highlight"
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

/** Monaco's name for a language the interface colors itself. */
export const monacoLanguage = (language: CodeLanguage) => (language === "bash" ? "shell" : language)

/** The name an editor's model goes by: the file's own, or one that says its language. */
export function modelName(path: string | undefined, language: CodeLanguage | undefined) {
  if (path) return path.slice(path.lastIndexOf("/") + 1) || "file"
  return language === "json" ? "value.json" : language === "bash" ? "script.sh" : "text.txt"
}

/**
 * Files Monaco knows no language for, given the nearest one it has, as Monaco's own language
 * points name files. Configurations that are keys and values under sections, TOML and
 * systemd's units among them, color as INI; shaders, whose languages are all C at heart, as C++.
 */
export const MORE_FILES: readonly {
  id: string
  extensions?: string[]
  filenames?: string[]
  filenamePatterns?: string[]
}[] = [
  {
    id: "ini",
    extensions: [
      ".conf",
      ".cfg",
      ".desktop",
      ".env",
      ".toml",
      ".service",
      ".socket",
      ".timer",
      ".target",
      ".mount",
      ".path",
    ],
    // `.env.local`, `.env.example` and the like.
    filenamePatterns: [".env.*"],
    filenames: ["Pipfile"],
  },
  {
    id: "cpp",
    extensions: [
      // GLSL, and its stages by their usual names.
      ".glsl",
      ".vert",
      ".frag",
      ".geom",
      ".tesc",
      ".tese",
      ".comp",
      // HLSL and Cg, Unity's shaders and compute shaders, Godot's, Unreal's and Metal's.
      ".hlsl",
      ".hlsli",
      ".fx",
      ".fxh",
      ".cginc",
      ".shader",
      ".compute",
      ".gdshader",
      ".usf",
      ".ush",
      ".metal",
    ],
  },
]

/**
 * The address of an editor's model. A JSON field's schema is matched to it, and Monaco tells a
 * file's language from the name at its end, such as `.py` or `Dockerfile`.
 */
export const modelUri = (id: string, name: string) =>
  `inmemory://thoric/editor/${id}/${encodeURIComponent(name)}`

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
 * The interface's code colors for Monaco's tokens, as `CodeTextarea` colors the same text. A
 * rule covers every token that starts with it, in every language, so `string` colors Python's
 * strings and YAML's alike; a longer rule, such as JSON's keys, wins over a shorter one.
 */
export const TOKEN_COLORS: Record<string, string> = {
  comment: "--code-comment",
  // A script's `#!` line.
  metatag: "--code-comment",
  keyword: "--code-keyword",
  // An INI file's `[section]`.
  "metatag.ini": "--code-keyword",
  tag: "--code-keyword",
  // Types, and the shell's builtin commands.
  type: "--code-command",
  predefined: "--code-command",
  string: "--code-string",
  regexp: "--code-string",
  "attribute.value": "--code-string",
  variable: "--code-variable",
  // An INI file's keys, named as JSON's are.
  key: "--code-variable",
  "attribute.name": "--code-variable",
  "string.key.json": "--code-variable",
  number: "--code-number",
  constant: "--code-number",
  delimiter: "--code-operator",
  operator: "--code-operator",
  // A heredoc's `<<`.
  constants: "--code-operator",
  // A command's options stay plain, as `CodeTextarea` leaves them.
  "attribute.name.shell": "--foreground",
}

/** A color as Monaco takes it, `RRGGBB` or `RRGGBBAA`, from the RGBA of a rendered color. */
export function hexColor([red, green, blue, alpha]: readonly number[], withHash = true) {
  const part = (value: number) => Math.round(value).toString(16).padStart(2, "0")
  const hex = `${part(red)}${part(green)}${part(blue)}${alpha < 255 ? part(alpha) : ""}`
  return withHash ? `#${hex}` : hex
}

/**
 * Monaco's token rules from the code colors, read through `color` as `RRGGBB`. Each says its
 * style, since a rule without one takes its parent's: an INI section would lean as a `#!` does.
 */
export const tokenRules = (color: (variable: string) => string) =>
  Object.entries(TOKEN_COLORS).map(([token, variable]) => ({
    token,
    foreground: color(variable).replace(/^#/, ""),
    fontStyle: variable === "--code-comment" ? "italic" : "",
  }))

/** The colors of nested brackets, outermost first, from the same palette as the tokens. */
export const BRACKET_COLORS = ["--code-operator", "--code-keyword", "--code-command"]

type Placing = Pick<
  CSSStyleDeclaration,
  | "transform"
  | "translate"
  | "scale"
  | "rotate"
  | "perspective"
  | "filter"
  | "backdropFilter"
  | "contain"
  | "willChange"
>

/**
 * Whether an element styled so places its `position: fixed` descendants against itself instead
 * of the window, as a dialog's centering translate or a glass surface's backdrop filter does.
 * Monaco places its suggestions and hovers by the window's coordinates, so under such an element
 * they would land elsewhere.
 */
export const holdsFixed = (style: Placing) =>
  [style.transform, style.translate, style.scale, style.rotate, style.perspective].some(
    (value) => value && value !== "none"
  ) ||
  [style.filter, style.backdropFilter].some((value) => value && value !== "none") ||
  /\b(paint|layout|strict|content)\b/.test(style.contain) ||
  /\b(transform|translate|scale|rotate|perspective|filter)\b/.test(style.willChange)

/** The editor's line height and the space above and below its text, in pixels. */
export const LINE_HEIGHT = 18
export const PADDING = 8

/** How tall an editor's text is with `rows` lines, the least a script field starts at. */
export const rowsHeight = (rows: number) => rows * LINE_HEIGHT + 2 * PADDING

/** How tall an editor is for content of `height` pixels: between its least and its most. */
export const editorHeight = (height: number, least: number, most: number) =>
  Math.round(Math.min(Math.max(height, least), most))
