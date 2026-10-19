import { useSyncExternalStore } from "react"

import type { ImportedBuild } from "./build-config"
import type { EndpointToken } from "./types"

export type Dialog =
  | { kind: "create"; image?: string }
  | { kind: "details"; id: string }
  | { kind: "settings"; id: string }
  | { kind: "snapshot"; id: string }
  | { kind: "delete"; id: string }
  | { kind: "upload"; id: string; files?: File[] }
  | { kind: "build"; config?: ImportedBuild }
  | { kind: "compose"; id: string }
  | { kind: "icon"; id: string }
  | { kind: "image"; id: string }
  | { kind: "image-delete"; id: string }
  | { kind: "import" }
  | { kind: "volume" }
  | { kind: "volume-files"; name: string }
  | { kind: "volume-import" }
  | { kind: "volume-copy"; name: string }
  | { kind: "volume-delete"; name: string }
  | { kind: "job"; id: string }
  | { kind: "log"; title: string; text: string; status?: string }
  | { kind: "mcp-endpoint"; name?: string }
  | { kind: "mcp-token"; token: EndpointToken }
  | { kind: "mcp-endpoint-confirm"; name: string; action: "replace" | "revoke" | "delete" }
  | { kind: "mcp-tools"; name: string }
  | { kind: "mcp-server"; container?: string }
  | { kind: "mcp-server-remove"; container: string; name: string }

type State = { dialog: Dialog | null; open: boolean; seq: number }

// One dialog at a time, as before: opening another replaces it. The last one stays mounted while
// it closes so its exit animation can run; `seq` remounts it with fresh form state when reopened.
let state: State = { dialog: null, open: false, seq: 0 }
const listeners = new Set<() => void>()
const emit = () => listeners.forEach((listener) => listener())

export function openDialog(dialog: Dialog) {
  state = { dialog, open: true, seq: state.seq + 1 }
  emit()
}

export function closeDialog() {
  if (!state.open) return
  state = { ...state, open: false }
  emit()
}

export function useDialogState() {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
    () => state
  )
}
