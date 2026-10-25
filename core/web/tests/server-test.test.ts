import { test } from "bun:test"
import assert from "node:assert/strict"

import { t } from "../src/lib/i18n"
import { PROTOCOL_VERSION } from "../src/lib/mcp-client"
import { OUTPUT_LINES, testServer } from "../src/lib/server-test"

type Sent = { id?: number; method?: string; params?: Record<string, unknown> }

/** The manager's end of the inspector socket, as the test plays it. */
class FakeSocket {
  readyState = 0
  sent: string[] = []
  closed = false
  onopen: (() => void) | null = null
  onmessage: ((event: { data: string }) => void) | null = null
  onclose: (() => void) | null = null
  send(text: string) {
    this.sent.push(text)
  }
  close() {
    this.closed = true
    this.readyState = 3
  }
  open() {
    this.readyState = 1
    this.onopen?.()
  }
  frame(frame: object) {
    this.onmessage?.({ data: JSON.stringify(frame) })
  }
  message(message: object) {
    this.frame({ type: "message", message: { jsonrpc: "2.0", ...message } })
  }
  request(method: string): Sent {
    const found = this.sent.slice(1).map((text) => JSON.parse(text) as Sent)
    return found.filter((message) => message.method === method).at(-1)!
  }
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 0))
const server = { name: "echo", command: "python3", args: ["/home/desktop/mcp/echo.py"] }

function start() {
  const socket = new FakeSocket()
  const run = testServer("c1", server, {
    socket: () => socket as unknown as WebSocket,
    version: "1",
  })
  return { socket, run }
}

test("a declaration is tried through the trial target and answers with its tools", async () => {
  const { socket, run } = start()
  let done = false
  void run.outcome.then(() => (done = true))
  socket.open()
  assert.deepEqual(JSON.parse(socket.sent[0]), { kind: "trial", container: "c1", server })
  socket.frame({ type: "ready", server: { kind: "trial", name: "echo" } })
  socket.message({
    id: socket.request("initialize").id,
    result: {
      protocolVersion: PROTOCOL_VERSION,
      capabilities: { tools: {} },
      serverInfo: { name: "echo", version: "1.2" },
    },
  })
  await tick()
  // Ready, but its tools are not read yet: the test is not over.
  assert.equal(done, false)
  socket.frame({ type: "stderr", line: "echo server listening on stdio", truncated: false })
  socket.message({
    id: socket.request("tools/list").id,
    result: { tools: [{ name: "echo", description: "Echo text" }] },
  })
  const outcome = await run.outcome
  assert.equal(outcome.ok, true)
  assert.deepEqual(outcome.info, { name: "echo", version: "1.2" })
  assert.deepEqual(
    outcome.tools.map((tool) => tool.name),
    ["echo"]
  )
  assert.deepEqual(outcome.output, ["echo server listening on stdio"])
  assert.ok(socket.closed, "the server is stopped once it answered")
})

test("a server that does not start says why, with the end of what it wrote", async () => {
  const { socket, run } = start()
  socket.open()
  socket.frame({ type: "ready", server: { kind: "trial", name: "echo" } })
  for (let line = 0; line < OUTPUT_LINES + 5; line++)
    socket.frame({ type: "stderr", line: `line ${line}`, truncated: false })
  socket.frame({ type: "closed", reason: "The server exited with code 127" })
  const outcome = await run.outcome
  assert.equal(outcome.ok, false)
  assert.equal(outcome.error, "The server exited with code 127")
  assert.equal(outcome.output.length, OUTPUT_LINES)
  assert.equal(outcome.output.at(-1), `line ${OUTPUT_LINES + 4}`)
})

test("a refused declaration fails before anything starts", async () => {
  const { socket, run } = start()
  socket.open()
  socket.frame({ type: "error", message: "Start the container before using its MCP servers" })
  const outcome = await run.outcome
  assert.deepEqual(outcome, {
    ok: false,
    error: "Start the container before using its MCP servers",
    tools: [],
    output: [],
  })
})

test("a server whose tool list fails is not a working server", async () => {
  const { socket, run } = start()
  socket.open()
  socket.frame({ type: "ready", server: { kind: "trial", name: "echo" } })
  socket.message({
    id: socket.request("initialize").id,
    result: { protocolVersion: PROTOCOL_VERSION, capabilities: { tools: {} } },
  })
  await tick()
  socket.message({
    id: socket.request("tools/list").id,
    error: { code: -32603, message: "No tools today" },
  })
  const outcome = await run.outcome
  assert.equal(outcome.ok, false)
  assert.equal(outcome.error, "No tools today")
})

test("a cancelled test stops the server and says so", async () => {
  const { socket, run } = start()
  socket.open()
  run.cancel()
  const outcome = await run.outcome
  assert.equal(outcome.error, t("inspector.cancelled"))
  assert.ok(socket.closed)
  // A second cancel, such as the dialog closing afterwards, changes nothing.
  run.cancel()
  assert.equal((await run.outcome).error, t("inspector.cancelled"))
})
