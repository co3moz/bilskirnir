import { test } from "bun:test"
import assert from "node:assert/strict"

import { codeLanguage, highlight, lineNumbers, type Token, type TokenKind } from "@/lib/highlight"

/** The kind of each piece, ignoring whitespace-only plain pieces, for readable expectations. */
const kinds = (tokens: Token[]) =>
  tokens
    .filter((token) => !(token.kind === "plain" && !token.text.trim()))
    .map((token) => [token.kind, token.text.trim()] as [TokenKind, string])

const SCRIPT = `#!/usr/bin/env bash
set -eu
# Install the tools the project needs.
export DEBIAN_FRONTEND=noninteractive
apt-get update && apt-get install -y --no-install-recommends git git-lfs
for tool in git git-lfs; do
  printf '%s\\n' "$tool \${HOME:-/root} $(command -v "$tool")" >> ~/tools.txt
done
case "$1" in
  start) echo done ;;
esac
cat > /etc/motd <<'EOF'
Welcome $USER, # not a comment
EOF
echo \\$HOME $((1 << 2)) 42
`

test("joining the pieces gives the code back, whatever it holds", () => {
  const samples = [
    SCRIPT,
    "",
    "$",
    "\\",
    'echo "unterminated',
    "echo 'unterminated",
    "cat <<EOF\nno end",
    "x=$(echo (",
    "${",
    '{"a": [1, -2.5e3, true, null, "\\"q\\""], "b": {}}',
    '{"broken": "line\n}',
    '[mcp_servers.a]\nurl = "http://x" # note\nb = { "c" = \'d\', n = 1.5 }',
    '[unclosed\nkey = "open',
  ]
  for (const sample of samples) {
    for (const language of ["bash", "json", "toml"] as const) {
      const tokens = highlight(sample, language)
      assert.equal(tokens.map((token) => token.text).join(""), sample, `${language}: ${sample}`)
      assert.ok(tokens.every((token) => token.text.length > 0))
    }
  }
})

test("a shell script colors comments, commands, keywords, strings and expansions", () => {
  const pieces = kinds(highlight(SCRIPT, "bash"))
  const has = (kind: TokenKind, text: string) =>
    assert.ok(
      pieces.some(([k, t]) => k === kind && t === text),
      `${kind} ${text} in ${JSON.stringify(pieces)}`
    )
  const lacks = (kind: TokenKind, text: string) =>
    assert.ok(!pieces.some(([k, t]) => k === kind && t === text), `no ${kind} ${text}`)

  has("comment", "#!/usr/bin/env bash")
  has("comment", "# Install the tools the project needs.")
  has("command", "set")
  has("command", "export")
  has("command", "apt-get")
  has("operator", "&&")
  has("keyword", "for")
  has("keyword", "in")
  has("keyword", "do")
  has("keyword", "done")
  has("keyword", "case")
  has("keyword", "esac")
  has("string", "'%s\\n'")
  has("variable", "$tool")
  has("variable", "${HOME:-/root}")
  has("variable", '$(command -v "$tool")')
  has("operator", ">>")
  has("variable", "$1")
  // `done` as an argument is a word, not the end of a loop.
  has("command", "echo")
  has("plain", "done")
  // A here document's body is text, also where it looks like code.
  has("operator", "<<")
  has("string", "'EOF'")
  has("string", "Welcome $USER, # not a comment\nEOF")
  lacks("comment", "# not a comment")
  // An escaped dollar is not an expansion, and an arithmetic shift is not a here document.
  has("plain", "\\$HOME")
  has("variable", "$((")
  has("number", "42")
})

test("an assignment before a command leaves the command in command position", () => {
  assert.deepEqual(kinds(highlight('LANG="C" make -j4', "bash")), [
    ["variable", "LANG"],
    ["plain", "="],
    ["string", '"C"'],
    ["command", "make"],
    ["plain", "-j4"],
  ])
})

test("a command substitution starts a new command", () => {
  assert.deepEqual(kinds(highlight("echo $(date +%s)", "bash")), [
    ["command", "echo"],
    ["variable", "$("],
    ["command", "date"],
    ["plain", "+%s"],
    ["variable", ")"],
  ])
})

test("JSON tells keys from string values", () => {
  assert.deepEqual(kinds(highlight('{"name": "Unity", "parts": [1, true, null]}', "json")), [
    ["operator", "{"],
    ["variable", '"name"'],
    ["operator", ":"],
    ["string", '"Unity"'],
    ["operator", ","],
    ["variable", '"parts"'],
    ["operator", ":"],
    ["operator", "["],
    ["number", "1"],
    ["operator", ","],
    ["keyword", "true"],
    ["operator", ","],
    ["keyword", "null"],
    ["operator", "]}"],
  ])
})

test("TOML tells table headers, keys and values apart", () => {
  const toml = `# Codex
[mcp_servers.chrome-dev]
url = "http://localhost:8080/mcp/chrome-dev"
http_headers = { "Authorization" = "Bearer x", retries = 3, on = true }`
  assert.deepEqual(kinds(highlight(toml, "toml")), [
    ["comment", "# Codex"],
    ["keyword", "[mcp_servers.chrome-dev]"],
    ["variable", "url"],
    ["operator", "="],
    ["string", '"http://localhost:8080/mcp/chrome-dev"'],
    ["variable", "http_headers"],
    ["operator", "="],
    ["operator", "{"],
    ["variable", '"Authorization"'],
    ["operator", "="],
    ["string", '"Bearer x"'],
    ["operator", ","],
    ["variable", "retries"],
    ["operator", "="],
    ["number", "3"],
    ["operator", ","],
    ["variable", "on"],
    ["operator", "="],
    ["keyword", "true"],
    ["operator", "}"],
  ])
})

test("a file opens colored as bash or JSON by its name or its first line", () => {
  assert.equal(codeLanguage("/home/desktop/setup.sh", "echo"), "bash")
  assert.equal(codeLanguage("/home/desktop/.bashrc", ""), "bash")
  assert.equal(codeLanguage("/home/desktop/run", "#!/usr/bin/env bash\necho"), "bash")
  assert.equal(codeLanguage("/usr/local/bin/tool", "#!/bin/sh -e\n"), "bash")
  assert.equal(codeLanguage("/home/desktop/package.JSON", "{}"), "json")
  assert.equal(codeLanguage("/home/desktop/notes.txt", "# not a shebang"), null)
  assert.equal(codeLanguage("/home/desktop/app.py", "#!/usr/bin/env python3\n"), null)
  assert.equal(codeLanguage("/home/desktop/fish", "#!/usr/bin/fish\n"), null)
})

test("the gutter numbers every line the field shows, a last empty one too", () => {
  assert.equal(lineNumbers(""), "1")
  assert.equal(lineNumbers("echo"), "1")
  assert.equal(lineNumbers("set -eu\necho"), "1\n2")
  assert.equal(lineNumbers("set -eu\necho\n"), "1\n2\n3")
})
