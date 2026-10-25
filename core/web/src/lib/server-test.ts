// Trying a desktop MCP server before it is saved: the inspector socket's trial target starts the
// declaration in a docker exec of its own, the client does the handshake and reads the lists,
// and the server stops again once the answer is in. Nothing is written to the desktop.
import { t } from "./i18n"
import type { ToolItem } from "./inspector"
import { InspectorClient, type ClientDeps, type InspectorState } from "./mcp-client"
import type { ServerDeclaration } from "./types"

export type TestOutcome = {
  ok: boolean
  /** Why the server did not answer. */
  error?: string
  info?: InspectorState["info"]
  protocolVersion?: string
  tools: ToolItem[]
  /** What the server wrote that is not a message: its stderr, and stdout that is not JSON. */
  output: string[]
}

/** The lines of output a test keeps, the last ones. */
export const OUTPUT_LINES = 30

/** How a test ended, from the inspector's state, or nothing while it goes on. */
export function outcomeOf(state: InspectorState): TestOutcome | null {
  const output = state.log
    .filter((entry) => entry.kind === "stderr" || entry.kind === "stdout")
    .map((entry) => entry.text)
    .slice(-OUTPUT_LINES)
  if (state.status === "closed")
    return { ok: false, error: state.error ?? t("inspector.disconnected"), tools: [], output }
  if (state.status !== "ready" || Object.values(state.loading).some(Boolean)) return null
  const answered = { info: state.info, protocolVersion: state.protocolVersion, output }
  if (state.listErrors.tool)
    return { ok: false, error: state.listErrors.tool, tools: [], ...answered }
  return { ok: true, tools: state.lists.tool, ...answered }
}

/** Start `server` in `container` without saving it; `cancel` stops it before it answers. */
export function testServer(container: string, server: ServerDeclaration, deps: ClientDeps) {
  const client = new InspectorClient({ kind: "trial", container, server }, deps)
  let cancel = () => {}
  const outcome = new Promise<TestOutcome>((resolve) => {
    const finish = (result: TestOutcome) => {
      unsubscribe()
      client.close()
      resolve(result)
    }
    const check = () => {
      const result = outcomeOf(client.getState())
      if (result) finish(result)
    }
    const unsubscribe = client.subscribe(check)
    cancel = () => finish({ ok: false, error: t("inspector.cancelled"), tools: [], output: [] })
    check()
  })
  return { outcome, cancel: () => cancel() }
}
