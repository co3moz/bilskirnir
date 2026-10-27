import { test } from "bun:test"
import assert from "node:assert/strict"

import { t } from "../src/lib/i18n"
import {
  InspectorClient,
  LOG_LIMIT,
  PROTOCOL_VERSION,
  RpcError,
  type InspectTarget,
} from "../src/lib/mcp-client"

type Sent = {
  id?: number
  method?: string
  params?: Record<string, unknown>
  [key: string]: unknown
}

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
  messages(): Sent[] {
    return this.sent.slice(1).map((text) => JSON.parse(text))
  }
  /** The last request of `method`. */
  request(method: string): Sent {
    const found = this.messages().filter((message) => message.method === method)
    assert.ok(found.length, `no ${method} in ${this.sent.join("\n")}`)
    return found[found.length - 1]
  }
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

function start(target: InspectTarget = { kind: "administrator" }, limits = {}) {
  const socket = new FakeSocket()
  const client = new InspectorClient(target, {
    socket: () => socket as unknown as WebSocket,
    version: "9.9.9",
    limits,
  })
  return { socket, client }
}

/** A client past the handshake with a server of `capabilities`, its lists answered empty. */
async function connected(capabilities: Record<string, unknown> = {}, limits = {}) {
  const { socket, client } = start({ kind: "administrator" }, limits)
  socket.open()
  socket.frame({ type: "ready", server: { kind: "administrator", path: "/mcp" } })
  const initialize = socket.request("initialize")
  socket.message({
    id: initialize.id,
    result: {
      protocolVersion: "2025-06-18",
      capabilities,
      serverInfo: { name: "test-server", version: "1.0" },
      instructions: "Be nice.",
    },
  })
  await tick()
  for (const message of socket.messages())
    if (message.method?.endsWith("/list"))
      socket.message({ id: message.id, result: { [listField(message.method)]: [] } })
  await tick()
  return { socket, client }
}

const listField = (method: string) =>
  method === "resources/templates/list" ? "resourceTemplates" : method.split("/")[0]

test("the first frame names the server, and the handshake asks for the latest revision", async () => {
  const target: InspectTarget = { kind: "server", container: "c1", name: "chrome" }
  const { socket, client } = start(target)
  assert.equal(client.getState().status, "connecting")
  socket.open()
  assert.deepEqual(JSON.parse(socket.sent[0]), target)
  socket.frame({ type: "ready", server: { kind: "server", name: "chrome" } })
  assert.equal(client.getState().status, "initializing")
  const initialize = socket.request("initialize")
  assert.equal(initialize.params?.protocolVersion, PROTOCOL_VERSION)
  assert.deepEqual(initialize.params?.clientInfo, {
    name: "thoric-inspector",
    version: "9.9.9",
  })
  socket.message({
    id: initialize.id,
    result: {
      protocolVersion: "2025-06-18",
      capabilities: {},
      serverInfo: { name: "s", version: "2" },
    },
  })
  await tick()
  const state = client.getState()
  assert.equal(state.status, "ready")
  assert.equal(state.protocolVersion, "2025-06-18")
  assert.deepEqual(state.info, { name: "s", version: "2" })
  assert.ok(socket.messages().some((message) => message.method === "notifications/initialized"))
  // A server without capabilities is asked for no list.
  assert.ok(!socket.messages().some((message) => message.method?.endsWith("/list")))
})

test("a list is read page by page from the servers that offer it", async () => {
  const { socket, client } = start()
  socket.open()
  socket.frame({ type: "ready", server: { kind: "administrator" } })
  socket.message({
    id: socket.request("initialize").id,
    result: { protocolVersion: PROTOCOL_VERSION, capabilities: { tools: {} } },
  })
  await tick()
  socket.message({
    id: socket.request("tools/list").id,
    result: { tools: [{ name: "one" }], nextCursor: "page-2" },
  })
  await tick()
  const second = socket.request("tools/list")
  assert.equal(second.params?.cursor, "page-2")
  socket.message({ id: second.id, result: { tools: [{ name: "two" }] } })
  await tick()
  assert.deepEqual(
    client.getState().lists.tool.map((tool) => tool.name),
    ["one", "two"]
  )
  assert.ok(!socket.messages().some((message) => message.method === "prompts/list"))
  // A list the server changed is read again.
  socket.message({ method: "notifications/tools/list_changed" })
  await tick()
  const again = socket.request("tools/list")
  socket.message({ id: again.id, result: { tools: [{ name: "three" }] } })
  await tick()
  assert.deepEqual(
    client.getState().lists.tool.map((tool) => tool.name),
    ["three"]
  )
})

test("a list the server refuses says why, and the others still come", async () => {
  const { socket, client } = start()
  socket.open()
  socket.frame({ type: "ready", server: { kind: "administrator" } })
  socket.message({
    id: socket.request("initialize").id,
    result: { protocolVersion: PROTOCOL_VERSION, capabilities: { tools: {}, prompts: {} } },
  })
  await tick()
  socket.message({ id: socket.request("tools/list").id, error: { code: -32603, message: "boom" } })
  socket.message({ id: socket.request("prompts/list").id, result: { prompts: [{ name: "p" }] } })
  await tick()
  assert.equal(client.getState().listErrors.tool, "boom")
  assert.equal(client.getState().lists.prompt.length, 1)
})

test("progress reaches the caller and keeps the request waiting", async () => {
  const { socket, client } = await connected({}, { request: 50 })
  const progress: unknown[] = []
  const answer = client.request(
    "tools/call",
    { name: "slow" },
    { onProgress: (value) => progress.push(value), timeout: 50 }
  )
  const call = socket.request("tools/call")
  const token = (call.params?._meta as Record<string, unknown>).progressToken
  assert.ok(token)
  await new Promise((resolve) => setTimeout(resolve, 30))
  socket.message({
    method: "notifications/progress",
    params: { progressToken: token, progress: 1, total: 2, message: "half" },
  })
  await new Promise((resolve) => setTimeout(resolve, 30))
  socket.message({ id: call.id, result: { content: [] } })
  assert.deepEqual(await answer, { content: [] })
  assert.deepEqual(progress, [{ progress: 1, total: 2, message: "half" }])
})

test("a cancelled request tells the server and stops waiting", async () => {
  const { socket, client } = await connected()
  const controller = new AbortController()
  const answer = client.request("tools/call", { name: "x" }, { signal: controller.signal })
  const call = socket.request("tools/call")
  controller.abort()
  await assert.rejects(answer, new Error(t("inspector.cancelled")))
  const cancelled = socket.request("notifications/cancelled")
  assert.equal(cancelled.params?.requestId, call.id)
  // A late answer is dropped.
  socket.message({ id: call.id, result: {} })
})

test("a request without an answer gives up and cancels itself", async () => {
  const { socket, client } = await connected()
  const answer = client.request("tools/call", {}, { timeout: 20 })
  await assert.rejects(answer, new Error(t("inspector.timedOut", { seconds: 0 })))
  assert.ok(socket.request("notifications/cancelled"))
})

test("an error answer rejects with its code", async () => {
  const { socket, client } = await connected()
  const answer = client.request("tools/call", { name: "missing" })
  socket.message({
    id: socket.request("tools/call").id,
    error: { code: -32602, message: "Unknown tool" },
  })
  await assert.rejects(answer, (error: unknown) => {
    assert.ok(error instanceof RpcError)
    assert.equal(error.code, -32602)
    assert.equal(error.message, "Unknown tool")
    return true
  })
})

test("the server's ping is answered and anything else it asks is refused", async () => {
  const { socket } = await connected()
  socket.message({ id: "s1", method: "ping" })
  socket.message({ id: "s2", method: "sampling/createMessage", params: {} })
  const answers = socket.messages().filter((message) => typeof message.id === "string")
  assert.deepEqual(answers[0], { jsonrpc: "2.0", id: "s1", result: {} })
  assert.equal((answers[1].error as { code: number }).code, -32601)
})

test("a request larger than the manager takes is refused before it is sent", async () => {
  const { socket, client } = await connected()
  const before = socket.sent.length
  await assert.rejects(
    client.request("tools/call", { data: "x".repeat(1_200_000) }),
    new Error(t("inspector.tooLarge"))
  )
  assert.equal(socket.sent.length, before)
})

test("output, server logs and the reason the server stopped are logged", async () => {
  const { socket, client } = await connected()
  const pending = client.request("tools/call", {})
  socket.frame({ type: "stderr", line: "warning", truncated: false })
  socket.message({ method: "notifications/message", params: { level: "error", data: { x: 1 } } })
  socket.frame({ type: "closed", reason: "The server exited with code 1" })
  await assert.rejects(pending, new Error("The server exited with code 1"))
  const state = client.getState()
  assert.equal(state.status, "closed")
  assert.equal(state.error, "The server exited with code 1")
  const kinds = state.log.map((entry) => entry.kind)
  assert.ok(
    kinds.includes("stderr") && kinds.includes("log") && kinds.includes("closed"),
    kinds.join()
  )
  assert.equal(state.log.find((entry) => entry.kind === "log")?.level, "error")
  assert.ok(socket.closed)
})

test("a refusal before the server started closes the inspector with it", () => {
  const { socket, client } = start()
  socket.open()
  socket.frame({ type: "error", message: "No MCP endpoint is called x" })
  assert.equal(client.getState().status, "closed")
  assert.equal(client.getState().error, "No MCP endpoint is called x")
})

test("a lost socket and a user's disconnect both close, only the first as an error", async () => {
  const lost = start()
  lost.socket.open()
  lost.socket.onclose?.()
  assert.equal(lost.client.getState().error, t("inspector.lost"))

  const { socket, client } = await connected()
  client.close()
  assert.equal(client.getState().status, "closed")
  assert.equal(client.getState().error, undefined)
  assert.ok(socket.closed)
  await assert.rejects(client.request("ping"), new Error(t("inspector.notConnected")))
})

test("the log keeps its newest entries", async () => {
  const { socket, client } = await connected()
  for (let line = 0; line < LOG_LIMIT + 20; line++)
    socket.frame({ type: "stdout", line: `${line}` })
  const log = client.getState().log
  assert.equal(log.length, LOG_LIMIT)
  assert.equal(log[log.length - 1].text, `${LOG_LIMIT + 19}`)
  client.clearLog()
  assert.equal(client.getState().log.length, 0)
})
