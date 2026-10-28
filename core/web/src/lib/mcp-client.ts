// The MCP inspector: an MCP client in the browser, joined to one server through the
// manager's inspector socket, `/api/mcp/inspect`. The first frame names the server; after it,
// every frame the client sends is one JSON-RPC message, and every frame back is an envelope
// with what the server wrote, its stderr, or why it stopped. See docs/API.md.
import { t } from "./i18n"
import { ITEM_KINDS, type ItemKind, type Lists } from "./inspector"
import { omit } from "./schema-form"
import type { ServerDeclaration } from "./types"

/** The inspector's WebSocket in the manager's API. */
export const INSPECT_PATH = "mcp/inspect"

/** The server an inspector opens, as its first frame names it. */
export type InspectTarget =
  | { kind: "server"; container: string; name: string }
  | { kind: "trial"; container: string; server: ServerDeclaration }
  | { kind: "endpoint"; name: string }
  | { kind: "administrator" }

/** What the manager started, as its `ready` frame describes it. */
export type ReadyServer = {
  kind: InspectTarget["kind"]
  container_id?: string
  container_name?: string
  name?: string
  source?: "image" | "container"
  transport?: "stdio" | "http"
  path?: string
}

export type LogEntry = {
  id: number
  /** Milliseconds since the epoch. */
  at: number
  /** `out` was sent to the server, `in` came from it; `note` is the inspector's own. */
  direction: "out" | "in" | "note"
  kind: "message" | "stdout" | "stderr" | "log" | "error" | "closed" | "ready"
  /** A method, `#id result` or a short note. */
  label: string
  text: string
  /** A server log message's level. */
  level?: string
  /** The text was longer than a log entry keeps. */
  cut?: boolean
}

export type Status = "connecting" | "initializing" | "ready" | "closed"

export type InspectorState = {
  status: Status
  /** Why the inspector closed, when it did not close at the user's request. */
  error?: string
  server?: ReadyServer
  info?: { name?: string; version?: string; title?: string }
  protocolVersion?: string
  capabilities: Record<string, unknown>
  instructions?: string
  lists: Lists
  /** Why a list could not be read. */
  listErrors: Partial<Record<ItemKind, string>>
  /** Whether a list is being read now. */
  loading: Partial<Record<ItemKind, boolean>>
  log: LogEntry[]
}

/** An error the server answered a request with. */
export class RpcError extends Error {
  code: number
  data?: unknown
  constructor(code: number, message: string, data?: unknown) {
    super(message)
    this.code = code
    this.data = data
  }
}

export type Progress = { progress: number; total?: number; message?: string }
export type RequestOptions = {
  signal?: AbortSignal
  onProgress?: (progress: Progress) => void
  /** Milliseconds without an answer or progress before the request gives up. */
  timeout?: number
}

export type ClientDeps = {
  /** Opens the socket; a test passes a fake. */
  socket(): WebSocket
  /** The interface's version, sent as the client's. */
  version: string
  /** Request limits in milliseconds, which tests shorten. */
  limits?: Partial<typeof LIMITS>
}

export const LIMITS = {
  /** The handshake and the lists. */
  request: 60_000,
  /** A call, a prompt or a read: as long as an endpoint lets a desktop server's call run. */
  call: 15 * 60_000,
}
/** The protocol revision the inspector asks for; the server may answer with another. */
export const PROTOCOL_VERSION = "2025-11-25"
/** The largest frame the manager takes from the browser. */
export const FRAME_LIMIT = 1_100_000
/** The entries the log keeps, and the text of each. */
export const LOG_LIMIT = 500
export const LOG_TEXT_LIMIT = 256 * 1024
const PAGES = 100
const LIST_METHODS: Record<ItemKind, { method: string; field: string; capability: string }> = {
  tool: { method: "tools/list", field: "tools", capability: "tools" },
  prompt: { method: "prompts/list", field: "prompts", capability: "prompts" },
  resource: { method: "resources/list", field: "resources", capability: "resources" },
  template: {
    method: "resources/templates/list",
    field: "resourceTemplates",
    capability: "resources",
  },
}
const CHANGES: Record<string, ItemKind[]> = {
  "notifications/tools/list_changed": ["tool"],
  "notifications/prompts/list_changed": ["prompt"],
  "notifications/resources/list_changed": ["resource", "template"],
}

type Message = {
  jsonrpc?: string
  id?: string | number | null
  method?: string
  params?: Record<string, unknown>
  result?: unknown
  error?: { code?: number; message?: string; data?: unknown }
}
type Pending = {
  id: number
  resolve(result: unknown): void
  reject(error: Error): void
  progressToken?: string
  onProgress?: (progress: Progress) => void
  timeout: number
  timer?: ReturnType<typeof setTimeout>
}

const emptyLists = (): Lists => ({ tool: [], prompt: [], resource: [], template: [] })

/** What a message is called in the log: its method, or the request it answers. */
function messageLabel(message: Message): string {
  if (message.method) return message.method
  if (message.error) return t("inspector.log.errorOf", { id: String(message.id) })
  return t("inspector.log.resultOf", { id: String(message.id) })
}

let opened = 0

export class InspectorClient {
  /** Tells this connection from the ones opened before it. */
  readonly id = ++opened
  readonly target: InspectTarget
  private readonly deps: ClientDeps
  private readonly limits: typeof LIMITS
  private socket: WebSocket | null
  private state: InspectorState
  private listeners = new Set<() => void>()
  private pending = new Map<number, Pending>()
  private lastId = 0
  private lastEntry = 0
  private ended = false

  constructor(target: InspectTarget, deps: ClientDeps) {
    this.target = target
    this.deps = deps
    this.limits = { ...LIMITS, ...deps.limits }
    this.state = {
      status: "connecting",
      capabilities: {},
      lists: emptyLists(),
      listErrors: {},
      loading: {},
      log: [],
    }
    const socket = deps.socket()
    this.socket = socket
    socket.onopen = () => {
      const frame = JSON.stringify(target)
      socket.send(frame)
      this.record("out", "message", t("inspector.log.open"), frame)
    }
    socket.onmessage = (event: MessageEvent) => this.frame(String(event.data))
    socket.onclose = () => this.end(t("inspector.lost"))
  }

  subscribe = (listener: () => void) => {
    this.listeners.add(listener)
    return () => {
      this.listeners.delete(listener)
    }
  }
  getState = () => this.state

  private update(patch: Partial<InspectorState>) {
    this.state = { ...this.state, ...patch }
    for (const listener of this.listeners) listener()
  }

  private record(
    direction: LogEntry["direction"],
    kind: LogEntry["kind"],
    label: string,
    text: string,
    level?: string
  ) {
    const cut = text.length > LOG_TEXT_LIMIT
    const entry: LogEntry = {
      id: ++this.lastEntry,
      at: Date.now(),
      direction,
      kind,
      label,
      text: cut ? text.slice(0, LOG_TEXT_LIMIT) : text,
      ...(level ? { level } : {}),
      ...(cut ? { cut } : {}),
    }
    const log = [...this.state.log, entry]
    this.update({ log: log.length > LOG_LIMIT ? log.slice(log.length - LOG_LIMIT) : log })
  }

  clearLog() {
    this.update({ log: [] })
  }

  /** One frame from the manager. */
  private frame(text: string) {
    let frame: { type?: string; [field: string]: unknown }
    try {
      frame = JSON.parse(text)
    } catch {
      return
    }
    switch (frame.type) {
      case "ready":
        this.update({ status: "initializing", server: frame.server as ReadyServer })
        this.record("note", "ready", t("inspector.log.started"), JSON.stringify(frame.server))
        void this.initialize()
        return
      case "message": {
        const message = frame.message as Message
        this.record("in", "message", messageLabel(message ?? {}), JSON.stringify(message))
        if (message && typeof message === "object") this.receive(message)
        return
      }
      case "stdout":
      case "stderr":
        this.record("in", frame.type, frame.type, String(frame.line ?? ""))
        return
      case "error":
        this.record("note", "error", t("inspector.log.refused"), String(frame.message ?? ""))
        // Before the server started, an error means it never will.
        if (this.state.status === "connecting") this.end(String(frame.message ?? ""))
        return
      case "closed":
        this.record("note", "closed", t("inspector.log.closed"), String(frame.reason ?? ""))
        this.end(String(frame.reason ?? t("inspector.lost")))
        return
    }
  }

  /** A JSON-RPC message from the server: an answer, a notification or a request of its own. */
  private receive(message: Message) {
    if (message.method === undefined) {
      const pending = typeof message.id === "number" ? this.pending.get(message.id) : undefined
      if (!pending) return
      this.settle(message.id as number)
      if (message.error)
        pending.reject(
          new RpcError(
            message.error.code ?? 0,
            message.error.message ?? t("inspector.rpcError"),
            message.error.data
          )
        )
      else pending.resolve(message.result)
      return
    }
    if (message.id !== undefined && message.id !== null) {
      // The inspector offers the server nothing of its own but an answer to its ping. The
      // refusal is the server's to read, so it is in English whatever the interface's language.
      this.send(
        message.method === "ping"
          ? { jsonrpc: "2.0", id: message.id, result: {} }
          : {
              jsonrpc: "2.0",
              id: message.id,
              error: {
                code: -32601,
                message: `The Thoric inspector does not support ${message.method}`,
              },
            }
      )
      return
    }
    const params = message.params ?? {}
    if (message.method === "notifications/progress") {
      for (const pending of this.pending.values())
        if (pending.progressToken !== undefined && pending.progressToken === params.progressToken) {
          this.arm(pending)
          pending.onProgress?.({
            progress: Number(params.progress),
            ...(typeof params.total === "number" ? { total: params.total } : {}),
            ...(typeof params.message === "string" ? { message: params.message } : {}),
          })
        }
    } else if (message.method === "notifications/message") {
      const data = typeof params.data === "string" ? params.data : JSON.stringify(params.data)
      const logger = typeof params.logger === "string" ? params.logger : undefined
      this.record(
        "in",
        "log",
        logger ?? t("inspector.log.server"),
        data ?? "",
        String(params.level ?? "")
      )
    } else if (CHANGES[message.method]) {
      void Promise.all(CHANGES[message.method].map((kind) => this.load(kind)))
    }
  }

  private send(message: Message) {
    const text = JSON.stringify(message)
    if (this.ended || this.socket?.readyState !== 1) throw new Error(t("inspector.notConnected"))
    if (new TextEncoder().encode(text).length > FRAME_LIMIT)
      throw new Error(t("inspector.tooLarge"))
    this.socket.send(text)
    this.record("out", "message", messageLabel(message), text)
  }

  /** Start the wait for an answer again; progress from the server starts it too. */
  private arm(pending: Pending) {
    clearTimeout(pending.timer)
    pending.timer = setTimeout(() => {
      if (this.pending.get(pending.id) !== pending) return
      this.cancel(pending.id, "Timed out")
      pending.reject(
        new Error(t("inspector.timedOut", { seconds: Math.round(pending.timeout / 1000) }))
      )
    }, pending.timeout)
  }

  private settle(id: number) {
    const pending = this.pending.get(id)
    if (pending) clearTimeout(pending.timer)
    this.pending.delete(id)
  }

  /** Stop waiting for a request and tell the server it is cancelled. */
  private cancel(id: number, reason: string) {
    this.settle(id)
    try {
      this.send({
        jsonrpc: "2.0",
        method: "notifications/cancelled",
        params: { requestId: id, reason },
      })
    } catch {
      // A closed inspector has nothing to tell.
    }
  }

  /** Send a request and wait for its answer, or its error. */
  request(method: string, params: Record<string, unknown> = {}, options: RequestOptions = {}) {
    return new Promise<unknown>((resolve, reject) => {
      if (options.signal?.aborted) return reject(new Error(t("inspector.cancelled")))
      const id = ++this.lastId
      const progressToken = options.onProgress ? `progress-${id}` : undefined
      const pending: Pending = {
        id,
        resolve,
        reject,
        progressToken,
        onProgress: options.onProgress,
        timeout: options.timeout ?? this.limits.request,
      }
      const meta = isRecord(params._meta) ? params._meta : {}
      try {
        this.send({
          jsonrpc: "2.0",
          id,
          method,
          params: progressToken ? { ...params, _meta: { ...meta, progressToken } } : params,
        })
      } catch (error) {
        return reject(error as Error)
      }
      this.pending.set(id, pending)
      this.arm(pending)
      options.signal?.addEventListener(
        "abort",
        () => {
          if (!this.pending.has(id)) return
          this.cancel(id, "The user cancelled the request")
          reject(new Error(t("inspector.cancelled")))
        },
        { once: true }
      )
    })
  }

  /** Send a notification, which nothing answers. */
  notify(method: string, params?: Record<string, unknown>) {
    this.send({ jsonrpc: "2.0", method, ...(params ? { params } : {}) })
  }

  private async initialize() {
    try {
      const result = (await this.request("initialize", {
        protocolVersion: PROTOCOL_VERSION,
        capabilities: {},
        clientInfo: { name: "thoric-inspector", version: this.deps.version },
      })) as Record<string, unknown>
      this.notify("notifications/initialized")
      const capabilities = isRecord(result?.capabilities) ? result.capabilities : {}
      this.update({
        status: "ready",
        info: isRecord(result?.serverInfo) ? (result.serverInfo as InspectorState["info"]) : {},
        protocolVersion:
          typeof result?.protocolVersion === "string" ? result.protocolVersion : undefined,
        capabilities,
        instructions: typeof result?.instructions === "string" ? result.instructions : undefined,
        // The lists the server offers are being read from the moment it is ready, so nobody
        // waiting for them takes a ready server with no list yet for one that offers none.
        loading: Object.fromEntries(
          ITEM_KINDS.filter((kind) => capabilities[LIST_METHODS[kind].capability]).map((kind) => [
            kind,
            true,
          ])
        ),
      })
      await this.refresh()
    } catch (error) {
      if (this.ended) return
      this.end(t("inspector.handshake", { error: (error as Error).message }))
    }
  }

  /** Read every list the server offers again. */
  refresh() {
    return Promise.all(ITEM_KINDS.map((kind) => this.load(kind)))
  }

  /** Read one list, page by page. A server without the capability has none. */
  private async load(kind: ItemKind) {
    const { method, field, capability } = LIST_METHODS[kind]
    const listErrors = omit(this.state.listErrors, kind)
    if (!this.state.capabilities[capability]) {
      this.update({ lists: { ...this.state.lists, [kind]: [] }, listErrors })
      return
    }
    this.update({ listErrors, loading: { ...this.state.loading, [kind]: true } })
    try {
      const items: unknown[] = []
      let cursor: unknown
      for (let page = 0; page < PAGES; page++) {
        const result = (await this.request(method, cursor ? { cursor } : {})) as Record<
          string,
          unknown
        >
        if (Array.isArray(result?.[field])) items.push(...(result[field] as unknown[]))
        cursor = result?.nextCursor
        if (typeof cursor !== "string" || !cursor) break
      }
      this.update({
        lists: { ...this.state.lists, [kind]: items.filter(isRecord) },
        loading: { ...this.state.loading, [kind]: false },
      })
    } catch (error) {
      if (this.ended) return
      this.update({
        lists: { ...this.state.lists, [kind]: [] },
        listErrors: { ...this.state.listErrors, [kind]: (error as Error).message },
        loading: { ...this.state.loading, [kind]: false },
      })
    }
  }

  /** Ask the server to send log messages of `level` and above. */
  setLogLevel(level: string) {
    return this.request("logging/setLevel", { level })
  }

  /** Close the inspector; the manager stops the server it started. */
  close() {
    this.end()
  }

  private end(error?: string) {
    if (this.ended) return
    this.ended = true
    const reason = error ?? t("inspector.disconnected")
    for (const [id, pending] of this.pending) {
      this.settle(id)
      pending.reject(new Error(reason))
    }
    if (this.socket) {
      this.socket.onopen = this.socket.onmessage = this.socket.onclose = null
      this.socket.close()
      this.socket = null
    }
    this.update({
      status: "closed",
      error,
      loading: {},
    })
  }
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value)
