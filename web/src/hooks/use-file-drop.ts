import * as React from "react"
import { toast } from "sonner"

const hasFiles = (event: React.DragEvent) => event.dataTransfer?.types.includes("Files")

/** Files dropped from the operating system; a dropped folder is refused, as uploads take files. */
export function droppedFiles(event: React.DragEvent | DragEvent) {
  const transfer = event.dataTransfer
  if (!transfer) return null
  if ([...transfer.items].some((item) => item.webkitGetAsEntry?.()?.isDirectory)) return null
  const files = [...transfer.files]
  return files.length ? files : null
}

/** Makes an element a drop target for files, with an `over` flag for its highlight. */
export function useFileDrop(onFiles: (files: File[]) => void) {
  const [over, setOver] = React.useState(false)
  const props = {
    onDragOver: (event: React.DragEvent) => {
      if (!hasFiles(event)) return
      event.preventDefault()
      event.stopPropagation()
      event.dataTransfer.dropEffect = "copy"
      setOver(true)
    },
    onDragLeave: (event: React.DragEvent) => {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setOver(false)
    },
    onDrop: (event: React.DragEvent) => {
      if (!hasFiles(event)) return
      event.preventDefault()
      event.stopPropagation()
      setOver(false)
      const files = droppedFiles(event)
      if (!files) toast.error("Choose files instead of a folder.")
      else onFiles(files)
    },
  }
  return { over, props }
}
