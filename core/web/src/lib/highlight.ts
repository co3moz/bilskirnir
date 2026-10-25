/** What a piece of code is, for its color. Text that is none of these is `plain`. */
export type TokenKind =
  "plain" | "comment" | "keyword" | "command" | "string" | "variable" | "number" | "operator"

export interface Token {
  kind: TokenKind
  text: string
}

export type CodeLanguage = "bash" | "json" | "toml"

const SHELL_FILES = new Set([
  ".bashrc",
  ".bash_profile",
  ".bash_aliases",
  ".bash_logout",
  ".profile",
])

/** The language a file is colored as when it is opened as text, or `null` for plain text. */
export function codeLanguage(path: string, text: string): CodeLanguage | null {
  const name = path.slice(path.lastIndexOf("/") + 1).toLowerCase()
  if (name.endsWith(".json")) return "json"
  if (/\.(sh|bash)$/.test(name) || SHELL_FILES.has(name)) return "bash"
  if (/^#![^\n]*\b(ba|da|z)?sh\b/.test(text)) return "bash"
  return null
}

/** "1\n2\n...", one number for each line of `text`, a last empty line included. */
export const lineNumbers = (text: string) =>
  Array.from({ length: text.split("\n").length }, (_, index) => index + 1).join("\n")

/** Splits code into colored pieces whose texts, joined, are the code again. */
export function highlight(code: string, language: CodeLanguage): Token[] {
  if (language === "json") return highlightJson(code)
  return language === "toml" ? highlightToml(code) : highlightBash(code)
}

/** Appends a piece, joining it to the previous one when both are the same kind. */
function push(tokens: Token[], kind: TokenKind, text: string) {
  if (!text) return
  const last = tokens[tokens.length - 1]
  if (last && last.kind === kind) last.text += text
  else tokens.push({ kind, text })
}

const KEYWORDS = new Set([
  "if",
  "then",
  "elif",
  "else",
  "fi",
  "for",
  "while",
  "until",
  "do",
  "done",
  "case",
  "esac",
  "select",
  "function",
  "time",
  "!",
  "[[",
  "]]",
  "{",
  "}",
])
/** Keywords after which the next word is a command again, as after `;`. */
const COMMAND_AFTER = new Set([
  "if",
  "then",
  "elif",
  "else",
  "while",
  "until",
  "do",
  "time",
  "!",
  "{",
])
/** Characters that end a word in a shell command. */
const BREAK = new Set([" ", "\t", "\n", ";", "&", "|", "<", ">", "(", ")", '"', "'", "`", "$"])
const NAME = /^[A-Za-z_][A-Za-z0-9_]*/
const SPECIAL_VARIABLE = /^[0-9@*#?$!-]/

/**
 * A shell script as bash reads it, closely enough to color it: comments, quoting, expansions,
 * keywords and command names in the places bash looks for them, operators, and here documents.
 * It never fails; text it does not recognize stays plain.
 */
export function highlightBash(code: string): Token[] {
  const tokens: Token[] = []
  let i = 0
  // Whether the next word is where bash expects a command name or a keyword.
  let commandStart = true
  // `for NAME in` and `case WORD in`: the `in` that follows is a keyword.
  let expectIn = 0
  // Here documents whose bodies start after the current line: their end words.
  let heredocs: { end: string; tabs: boolean }[] = []
  // Open `$(` and `$((`, closed by `)` and `))`.
  const substitutions: string[] = []
  // Right after `NAME=`: a quoted value or an expansion still leaves the command name to come.
  let assignmentValue = false
  const wordDone = () => {
    if (!assignmentValue) commandStart = false
  }

  const readHeredocBodies = () => {
    for (const { end, tabs } of heredocs) {
      while (i < code.length) {
        const lineEnd = code.indexOf("\n", i)
        const stop = lineEnd === -1 ? code.length : lineEnd + 1
        const line = code.slice(i, lineEnd === -1 ? code.length : lineEnd)
        push(tokens, "string", code.slice(i, stop))
        i = stop
        if ((tabs ? line.replace(/^\t+/, "") : line) === end) break
      }
    }
    heredocs = []
  }

  const variableAt = (at: number): number => {
    // Returns the end of the expansion that starts with `$` at `at`, or `at` when there is none.
    const next = code[at + 1]
    if (next === "{") {
      let depth = 0
      for (let j = at + 1; j < code.length; j++) {
        if (code[j] === "{") depth++
        else if (code[j] === "}" && --depth === 0) return j + 1
        else if (code[j] === "\n") return j
      }
      return code.length
    }
    const name = NAME.exec(code.slice(at + 1))
    if (name) return at + 1 + name[0].length
    if (next !== undefined && SPECIAL_VARIABLE.test(next)) return at + 2
    return at
  }

  const doubleQuoted = () => {
    // From the opening `"`: the string with its expansions colored as variables.
    let start = i
    i++
    while (i < code.length && code[i] !== '"') {
      if (code[i] === "\\") {
        i += 2
      } else if (code[i] === "$" && code[i + 1] === "(") {
        push(tokens, "string", code.slice(start, i))
        const end = matchingParen(code, i + 1)
        push(tokens, "variable", code.slice(i, end))
        i = start = end
      } else if (code[i] === "$" && variableAt(i) > i) {
        push(tokens, "string", code.slice(start, i))
        const end = variableAt(i)
        push(tokens, "variable", code.slice(i, end))
        i = start = end
      } else if (code[i] === "`") {
        push(tokens, "string", code.slice(start, i))
        const close = code.indexOf("`", i + 1)
        const end = close === -1 ? code.length : close + 1
        push(tokens, "variable", code.slice(i, end))
        i = start = end
      } else {
        i++
      }
    }
    i = Math.min(i + 1, code.length)
    push(tokens, "string", code.slice(start, i))
  }

  while (i < code.length) {
    const c = code[i]
    const rest = code.slice(i)

    if (c === "\n") {
      push(tokens, "plain", c)
      i++
      commandStart = true
      expectIn = 0
      assignmentValue = false
      readHeredocBodies()
      continue
    }
    if (c === " " || c === "\t") {
      const end = /^[ \t]+/.exec(rest)![0].length
      push(tokens, "plain", rest.slice(0, end))
      i += end
      assignmentValue = false
      continue
    }
    if (c === "\\") {
      // An escaped character is a literal one; a backslash before a newline continues the line.
      push(tokens, "plain", code.slice(i, i + 2))
      i += 2
      continue
    }
    if (c === "#") {
      const lineEnd = code.indexOf("\n", i)
      const end = lineEnd === -1 ? code.length : lineEnd
      push(tokens, "comment", code.slice(i, end))
      i = end
      continue
    }
    if (c === "'") {
      const close = code.indexOf("'", i + 1)
      const end = close === -1 ? code.length : close + 1
      push(tokens, "string", code.slice(i, end))
      i = end
      wordDone()
      continue
    }
    if (c === '"') {
      doubleQuoted()
      wordDone()
      continue
    }
    if (c === "`") {
      const close = code.indexOf("`", i + 1)
      const end = close === -1 ? code.length : close + 1
      push(tokens, "variable", code.slice(i, end))
      i = end
      wordDone()
      continue
    }
    if (c === "$") {
      if (code[i + 1] === "'") {
        // ANSI-C quoting: $'...' with backslash escapes.
        let j = i + 2
        while (j < code.length && code[j] !== "'") j += code[j] === "\\" ? 2 : 1
        const end = Math.min(j + 1, code.length)
        push(tokens, "string", code.slice(i, end))
        i = end
        wordDone()
        continue
      }
      if (code[i + 1] === "(") {
        const opener = code[i + 2] === "(" ? "$((" : "$("
        push(tokens, "variable", opener)
        substitutions.push(opener === "$((" ? "))" : ")")
        i += opener.length
        commandStart = opener === "$("
        continue
      }
      const end = variableAt(i)
      if (end > i) {
        push(tokens, "variable", code.slice(i, end))
        i = end
        wordDone()
        continue
      }
    }
    if (c === ")" && substitutions.length) {
      const close = substitutions[substitutions.length - 1]
      if (rest.startsWith(close)) {
        substitutions.pop()
        push(tokens, "variable", close)
        i += close.length
        commandStart = false
        continue
      }
    }
    const arithmetic = substitutions[substitutions.length - 1] === "))"
    if (c === "<" && rest.startsWith("<<") && !rest.startsWith("<<<") && !arithmetic) {
      // A here document: the end word may be quoted, and its body is read after this line.
      const tabs = rest.startsWith("<<-")
      const operator = tabs ? "<<-" : "<<"
      push(tokens, "operator", operator)
      i += operator.length
      const spaces = /^[ \t]*/.exec(code.slice(i))![0]
      push(tokens, "plain", spaces)
      i += spaces.length
      const word = /^(\\?[A-Za-z0-9_.-]+|'[^'\n]*'|"[^"\n]*")/.exec(code.slice(i))
      if (word) {
        push(tokens, "string", word[0])
        i += word[0].length
        heredocs.push({ end: word[0].replace(/^\\/, "").replace(/^(['"])(.*)\1$/, "$2"), tabs })
      }
      continue
    }
    const operator = /^(&&|\|\||;;|\|&|>>|<<<|&>|>&|<&|[;&|<>()])/.exec(rest)
    if (operator) {
      push(tokens, "operator", operator[0])
      i += operator[0].length
      assignmentValue = false
      if (!/^[<>]|^&>|^>&|^<&/.test(operator[0])) commandStart = true
      continue
    }

    // A word: everything up to the next character that ends one.
    let end = i
    while (end < code.length && !BREAK.has(code[end])) {
      end += code[end] === "\\" ? 2 : 1
    }
    end = Math.min(end, code.length)
    if (end === i) {
      // A lone `$` that starts nothing.
      push(tokens, "plain", c)
      i++
      continue
    }
    const word = code.slice(i, end)
    const assignment = /^([A-Za-z_][A-Za-z0-9_]*)(\[[^\]]*\])?\+?=/.exec(word)
    if (commandStart && KEYWORDS.has(word)) {
      push(tokens, "keyword", word)
      commandStart = COMMAND_AFTER.has(word)
      expectIn = word === "for" || word === "case" || word === "select" ? 2 : 0
    } else if (expectIn > 0 && word === "in") {
      push(tokens, "keyword", word)
      expectIn = 0
      commandStart = false
    } else if (commandStart && assignment) {
      push(tokens, "variable", assignment[1])
      push(tokens, "plain", word.slice(assignment[1].length))
      assignmentValue = true
    } else if (assignmentValue) {
      // The rest of an assignment's value, as in NAME="a"b.
      push(tokens, "plain", word)
    } else if (commandStart) {
      push(tokens, "command", word)
      commandStart = false
    } else if (/^-?\d+$/.test(word)) {
      push(tokens, "number", word)
    } else {
      push(tokens, "plain", word)
    }
    if (expectIn === 2 && !KEYWORDS.has(word)) expectIn = 1
    i = end
  }
  return tokens
}

/** The end of `(...)` that starts at `open`, counting nested parentheses on any line. */
function matchingParen(code: string, open: number): number {
  if (code[open] === "`") {
    const close = code.indexOf("`", open + 1)
    return close === -1 ? code.length : close + 1
  }
  let depth = 0
  for (let j = open; j < code.length; j++) {
    if (code[j] === "(") depth++
    else if (code[j] === ")" && --depth === 0) return j + 1
  }
  return code.length
}

/** JSON: keys apart from string values, numbers, and the literals `true`, `false` and `null`. */
export function highlightJson(code: string): Token[] {
  const tokens: Token[] = []
  let i = 0
  while (i < code.length) {
    const c = code[i]
    if (c === '"') {
      let j = i + 1
      while (j < code.length && code[j] !== '"' && code[j] !== "\n") j += code[j] === "\\" ? 2 : 1
      const end = Math.min(code[j] === '"' ? j + 1 : j, code.length)
      const key = /^\s*:/.test(code.slice(end))
      push(tokens, key ? "variable" : "string", code.slice(i, end))
      i = end
      continue
    }
    const number = /^-?\d+(\.\d+)?([eE][+-]?\d+)?/.exec(code.slice(i))
    if (number && !/[A-Za-z0-9_]/.test(code[i - 1] ?? "")) {
      push(tokens, "number", number[0])
      i += number[0].length
      continue
    }
    const literal = /^(true|false|null)\b/.exec(code.slice(i))
    if (literal) {
      push(tokens, "keyword", literal[0])
      i += literal[0].length
      continue
    }
    if ("{}[]:,".includes(c)) {
      push(tokens, "operator", c)
      i++
      continue
    }
    push(tokens, "plain", c)
    i++
  }
  return tokens
}

/** The end of a string that opens at `open` with its quote; at the end of the line if unclosed. */
function stringEnd(code: string, open: number): number {
  const quote = code[open]
  let j = open + 1
  // A literal string, in single quotes, has no escapes.
  while (j < code.length && code[j] !== quote && code[j] !== "\n")
    j += quote === '"' && code[j] === "\\" ? 2 : 1
  return Math.min(code[j] === quote ? j + 1 : j, code.length)
}

/**
 * TOML: table headers, keys apart from string values, comments, numbers and `true`/`false`.
 * Multi-line strings are colored line by line, which is close enough for a configuration snippet.
 */
export function highlightToml(code: string): Token[] {
  const tokens: Token[] = []
  let i = 0
  // Whether only blanks stand between the start of the line and `i`, where a header may start.
  let lineStart = true
  while (i < code.length) {
    const c = code[i]
    if (c === " " || c === "\t" || c === "\r" || c === "\n") {
      push(tokens, "plain", c)
      if (c === "\n") lineStart = true
      i++
      continue
    }
    const atStart = lineStart
    lineStart = false
    if (c === "#") {
      const end = code.indexOf("\n", i)
      const stop = end === -1 ? code.length : end
      push(tokens, "comment", code.slice(i, stop))
      i = stop
      continue
    }
    const header = atStart && c === "[" ? /^\[\[?[^\]\n]*\]?\]?/.exec(code.slice(i)) : null
    if (header) {
      push(tokens, "keyword", header[0])
      i += header[0].length
      continue
    }
    if (c === '"' || c === "'") {
      const end = stringEnd(code, i)
      push(tokens, /^[ \t]*[=.]/.test(code.slice(end)) ? "variable" : "string", code.slice(i, end))
      i = end
      continue
    }
    const word = /^[A-Za-z0-9_+.:-]+/.exec(code.slice(i))
    if (word) {
      const text = word[0]
      const key = /^[ \t]*=/.test(code.slice(i + text.length))
      const kind: TokenKind = key
        ? "variable"
        : text === "true" || text === "false"
          ? "keyword"
          : /^[+-]?\d/.test(text)
            ? "number"
            : "plain"
      push(tokens, kind, text)
      i += text.length
      continue
    }
    push(tokens, "=,{}[]".includes(c) ? "operator" : "plain", c)
    i++
  }
  return tokens
}
