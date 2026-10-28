import { useSyncExternalStore } from "react"

import { wsUrl } from "./api"
import { targetKey } from "./inspector"
import { INSPECT_PATH, InspectorClient, type InspectTarget } from "./mcp-client"
import { useSettings } from "./queries"
import { navigate } from "./route"

/** The server open in the inspector, and the one its picker shows. */
export type InspectorSession = { client: InspectorClient | null; picked?: string }

// The inspector page is one of several MCP pages, and the endpoints and desktop servers pages open
// servers in it. Its server lives here, so it stays open while the user moves between them.
let session: InspectorSession = { client: null }
const listeners = new Set<() => void>()

function update(next: InspectorSession) {
  session = next
  for (const listener of listeners) listener()
}

/** Opens `target` in the inspector, and ends the server open before it. */
export function openInspector(target: InspectTarget, version: string) {
  session.client?.close()
  update({
    client: new InspectorClient(target, {
      socket: () => new WebSocket(wsUrl(INSPECT_PATH)),
      version,
    }),
    picked: targetKey(target),
  })
}

/** Opens a server on the inspector page, from a row of another MCP page. */
export function useOpenInInspector() {
  const version = useSettings().data?.version ?? ""
  return (target: InspectTarget) => {
    openInspector(target, version)
    navigate({ page: "mcp-inspector" })
  }
}

/** Chooses the server the inspector connects to next. */
export const pickInspectorTarget = (picked: string) => update({ ...session, picked })

/** Ends the inspector's server and forgets the choice, as the dashboard leaves the MCP pages. */
export function closeInspector() {
  if (!session.client && session.picked === undefined) return
  session.client?.close()
  update({ client: null })
}

const subscribe = (listener: () => void) => {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export const useInspectorSession = () => useSyncExternalStore(subscribe, () => session)
