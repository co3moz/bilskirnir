import { useSyncExternalStore } from "react"

export type Dialog =
  | { kind: "create"; image?: string }
  | { kind: "details"; id: string }
  | { kind: "settings"; id: string }
  | { kind: "snapshot"; id: string }
  | { kind: "delete"; id: string }
  | { kind: "upload"; id: string; files?: File[] }
  | { kind: "build" }
  | { kind: "compose"; id: string }
  | { kind: "import" }
  | { kind: "volume" }
  | { kind: "volume-files"; name: string }
  | { kind: "volume-import" }
  | { kind: "volume-copy"; name: string }
  | { kind: "job"; id: string }
  | { kind: "log"; title: string; text: string; status?: string }

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
