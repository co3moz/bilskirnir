import { test } from "bun:test"
import assert from "node:assert/strict"

import { highlight } from "@/lib/highlight"
import {
  MCP_CLIENTS,
  mcpClient,
  mcpRemote,
  shellWord,
  wslBridge,
  type ConnectTarget,
} from "@/lib/mcp-connect"

const endpoint: ConnectTarget = {
  name: "chrome-dev",
  url: "http://localhost:8080/mcp/chrome-dev",
  path: "/mcp/chrome-dev",
  token: "s3cr3t",
}
const tokenless: ConnectTarget = { ...endpoint, token: null }
const admin: ConnectTarget = {
  name: "bilskirnir",
  url: "https://bilskirnir.lan/mcp",
  path: "/mcp",
  token: "admin",
}

const everything = (target: ConnectTarget) =>
  MCP_CLIENTS.flatMap((client) => [
    ...client.snippets(target).map((snippet) => snippet.text),
    ...(client.install ? [decodeURIComponent(client.install(target))] : []),
  ])

test("every client has settings, each id once, and unknown ids fall back to the first", () => {
  assert.equal(new Set(MCP_CLIENTS.map((client) => client.id)).size, MCP_CLIENTS.length)
  assert.equal(MCP_CLIENTS.at(-1)?.id, "generic")
  for (const client of MCP_CLIENTS) {
    assert.ok(client.snippets(endpoint).length > 0, client.id)
    assert.match(client.docs, /^https:\/\//, client.id)
  }
  assert.equal(mcpClient("codex").id, "codex")
  assert.equal(mcpClient("telepathy").id, MCP_CLIENTS[0].id)
  assert.equal(mcpClient(null).id, MCP_CLIENTS[0].id)
})

test("JSON settings parse and name the server and its URL", () => {
  for (const client of MCP_CLIENTS)
    for (const snippet of client.snippets(endpoint)) {
      if (snippet.language !== "json") continue
      const text = JSON.stringify(JSON.parse(snippet.text))
      assert.ok(text.includes('"chrome-dev"'), `${client.id} ${snippet.kind}`)
      // A bridge reaches the manager from inside, by the path alone.
      if (snippet.kind !== "wsl") assert.ok(text.includes(endpoint.url), `${client.id}`)
    }
})

test("the token goes into every client's settings, and nowhere without a header", () => {
  for (const client of MCP_CLIENTS) {
    const texts = client
      .snippets(endpoint)
      .filter((snippet) => snippet.kind !== "wsl")
      .map((snippet) => snippet.text)
    assert.ok(
      texts.every((text) => text.includes("Bearer s3cr3t")),
      client.id
    )
  }
  for (const text of everything(tokenless)) {
    assert.ok(!text.includes("Authorization"), text)
    assert.ok(!text.includes("Bearer"), text)
  }
})

test("commands quote what a shell would split", () => {
  const claude = mcpClient("claude-code").snippets(endpoint)[0]
  assert.equal(
    claude.text,
    'claude mcp add --transport http chrome-dev http://localhost:8080/mcp/chrome-dev --header "Authorization: Bearer s3cr3t"'
  )
  // A token with characters double quotes would expand gets single quotes, a quote of its own too.
  const odd = mcpClient("claude-code").snippets({ ...endpoint, token: "it's$x" })[0]
  assert.ok(odd.text.endsWith(`'Authorization: Bearer it'\\''s$x'`), odd.text)
  assert.equal(shellWord("it's"), `"it's"`)
  assert.equal(shellWord("/mcp/a-b"), "/mcp/a-b")
  assert.equal(
    mcpClient("claude-code").snippets(tokenless)[0].text,
    "claude mcp add --transport http chrome-dev http://localhost:8080/mcp/chrome-dev"
  )
})

test("Codex gets a TOML table with the header, quoted as TOML quotes", () => {
  const [codex] = mcpClient("codex").snippets({ ...endpoint, token: 'a"b' })
  assert.equal(codex.language, "toml")
  assert.equal(
    codex.text,
    [
      "[mcp_servers.chrome-dev]",
      'url = "http://localhost:8080/mcp/chrome-dev"',
      'http_headers = { "Authorization" = "Bearer a\\"b" }',
    ].join("\n")
  )
  // Colored as a header, a key and a string, so the highlighter reads it as TOML.
  const kinds = highlight(codex.text, "toml").map((token) => token.kind)
  assert.deepEqual(kinds.slice(0, 3), ["keyword", "plain", "variable"])
  // Its command takes a token only from an environment variable, so it is offered without one.
  assert.equal(mcpClient("codex").snippets(endpoint).length, 1)
  assert.equal(
    mcpClient("codex").snippets(tokenless)[0].text,
    "codex mcp add chrome-dev --url http://localhost:8080/mcp/chrome-dev"
  )
})

test("OpenCode is kept from starting a sign-in only when a token is sent", () => {
  const server = (target: ConnectTarget) =>
    JSON.parse(mcpClient("opencode").snippets(target)[0].text).mcp["chrome-dev"]
  assert.equal(server(endpoint).oauth, false)
  assert.equal("oauth" in server(tokenless), false)
})

test("install links carry the same server the settings do", () => {
  const vscode = mcpClient("vscode").install!(endpoint)
  assert.ok(vscode.startsWith("vscode:mcp/install?"))
  assert.deepEqual(JSON.parse(decodeURIComponent(vscode.slice("vscode:mcp/install?".length))), {
    name: "chrome-dev",
    type: "http",
    url: endpoint.url,
    headers: { Authorization: "Bearer s3cr3t" },
  })
  const cursor = new URL(mcpClient("cursor").install!(endpoint))
  assert.equal(cursor.protocol, "cursor:")
  assert.equal(cursor.searchParams.get("name"), "chrome-dev")
  assert.deepEqual(JSON.parse(atob(cursor.searchParams.get("config")!)), {
    url: endpoint.url,
    headers: { Authorization: "Bearer s3cr3t" },
  })
  // A token beyond Latin-1 still encodes, as UTF-8.
  const wide = new URL(mcpClient("cursor").install!({ ...endpoint, token: "ğ" }))
  const bytes = Uint8Array.from(atob(wide.searchParams.get("config")!), (c) => c.charCodeAt(0))
  assert.equal(JSON.parse(new TextDecoder().decode(bytes)).headers.Authorization, "Bearer ğ")
})

test("the WSL bridge reaches the endpoint by its path and never carries a token", () => {
  assert.deepEqual(wslBridge(admin).args, [
    "-d",
    "Bilskirnir",
    "--",
    "docker",
    "exec",
    "-i",
    "bilskirnir-manager",
    "python3",
    "/app/mcp_bridge.py",
  ])
  const args = wslBridge(endpoint).args
  assert.ok(args.includes("MCP_BRIDGE_URL=http://127.0.0.1:8080/mcp/chrome-dev"))
  assert.ok(!JSON.stringify(wslBridge(endpoint)).includes("s3cr3t"))
})

test("mcp-remote passes the header through its environment and allows plain HTTP on the LAN", () => {
  assert.deepEqual(mcpRemote(endpoint), {
    command: "npx",
    args: ["-y", "mcp-remote", endpoint.url, "--header", "Authorization:${AUTH_HEADER}"],
    env: { AUTH_HEADER: "Bearer s3cr3t" },
  })
  assert.deepEqual(mcpRemote({ ...tokenless, url: "http://192.168.1.5:8080/mcp/x" }).args, [
    "-y",
    "mcp-remote",
    "http://192.168.1.5:8080/mcp/x",
    "--allow-http",
  ])
  assert.ok(!mcpRemote(admin).args.includes("--allow-http"))
})
