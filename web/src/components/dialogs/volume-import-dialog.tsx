import * as React from "react"
import { RiFileZipLine } from "@remixicon/react"
import { toast } from "sonner"
import { cn } from "cn"

import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Progress } from "@/components/ui/progress"
import { Spinner } from "@/components/ui/spinner"
import { useFileDrop } from "@/hooks/use-file-drop"
import { sessionExpired } from "@/lib/api"
import { ArchiveImport } from "@/lib/archives"
import { closeDialog, openDialog } from "@/lib/dialogs"
import { refreshAll, useSettings } from "@/lib/queries"
import { fileSize } from "@/lib/uploads"
import { VOLUME_NAME, volumeArchiveName } from "@/lib/volume-files"
import type { DialogProps } from "./side-drawer"

/** Unpacks a .tar.gz or .tar into a new volume. */
export function VolumeImportDialog({ open, onOpenChange }: DialogProps) {
  const limit = useSettings().data?.archive_limit_bytes ?? 0
  const [, render] = React.useReducer((count: number) => count + 1, 0)
  const [upload] = React.useState(
    () =>
      new ArchiveImport({
        path: "volumes/import",
        createRequest: () => new XMLHttpRequest(),
        onChange: render,
        onUnauthorized: sessionExpired,
      })
  )
  React.useEffect(() => () => upload.cancel(), [upload])
  // A closing dialog stays mounted for its animation, so stop the upload as soon as it closes.
  React.useEffect(() => {
    if (!open) upload.cancel()
  }, [open, upload])
  const [file, setFile] = React.useState<File | null>(null)
  const [name, setName] = React.useState("")
  const [error, setError] = React.useState("")
  const input = React.useRef<HTMLInputElement>(null)
  const field = React.useId()

  const choose = (files: File[]) => {
    const [chosen] = files
    if (!chosen || upload.running) return
    if (files.length > 1) {
      setError("Choose one archive at a time.")
      return
    }
    setFile(chosen)
    setError("")
    setName((current) => current || volumeArchiveName(chosen.name))
  }
  const drop = useFileDrop(choose)

  const submit = async (event: React.FormEvent) => {
    event.preventDefault()
    if (!file) {
      setError("Choose an archive first.")
      return
    }
    setError("")
    const job = await upload.start(file, { name: name.trim() }, limit)
    if (!job) return
    closeDialog()
    toast.success(`Importing ${name.trim()}.`, {
      description: "The archive arrived. Its last files are being written.",
    })
    void refreshAll()
    openDialog({ kind: "job", id: job })
  }

  const running = upload.running
  const status =
    upload.status === "checking"
      ? "Writing the last files…"
      : `${fileSize(upload.loaded)} of ${fileSize(upload.size)} uploaded`
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <form className="flex flex-col gap-4" onSubmit={submit}>
          <DialogHeader>
            <DialogTitle>Import volume</DialogTitle>
            <DialogDescription>
              Unpacks an archive exported from a volume into a new one.
            </DialogDescription>
          </DialogHeader>
          <FieldGroup className="gap-4">
            <button
              type="button"
              {...drop.props}
              disabled={running}
              onClick={() => input.current?.click()}
              className={cn(
                "flex flex-col items-center justify-center gap-1 rounded-lg border border-dashed px-4 py-6 text-sm transition-colors outline-none hover:bg-muted/50 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50",
                drop.over && "border-primary bg-primary/5"
              )}
            >
              <RiFileZipLine className="mb-1 size-5 text-muted-foreground" />
              {file ? (
                <>
                  <strong className="max-w-full truncate font-medium">{file.name}</strong>
                  <span className="text-xs text-muted-foreground">{fileSize(file.size)}</span>
                </>
              ) : (
                <>
                  <strong className="font-medium">Drop an archive here or choose a file</strong>
                  <span className="text-xs text-muted-foreground">
                    .tar.gz or .tar · up to {fileSize(limit)} unpacked
                  </span>
                </>
              )}
            </button>
            <input
              ref={input}
              type="file"
              hidden
              accept=".gz,.tgz,.tar,application/gzip,application/x-tar"
              onChange={(event) => {
                choose([...(event.target.files ?? [])])
                event.target.value = ""
              }}
            />
            <Field>
              <FieldLabel htmlFor={`${field}-name`}>Volume name</FieldLabel>
              <Input
                id={`${field}-name`}
                required
                pattern={VOLUME_NAME.source}
                title="Letters, digits, dots, dashes and underscores, starting with a letter or digit"
                placeholder="unity-projects"
                disabled={running}
                value={name}
                onChange={(event) => setName(event.target.value)}
                className="font-mono"
              />
              <FieldDescription>
                A new volume; an existing one is never replaced. Files belong to the desktop user.
              </FieldDescription>
            </Field>
            {upload.status !== "idle" && upload.status !== "started" && upload.size > 0 && (
              <div role="status" aria-live="polite" className="flex flex-col gap-1.5">
                <Progress
                  value={(upload.loaded * 100) / Math.max(upload.size, 1)}
                  aria-label="Archive upload progress"
                />
                <span className="text-xs text-muted-foreground">
                  {upload.status === "cancelled" ? "Cancelled" : status}
                </span>
              </div>
            )}
            <FieldError>{error || (upload.status === "failed" ? upload.error : "")}</FieldError>
          </FieldGroup>
          <DialogFooter>
            {running ? (
              <Button type="button" variant="outline" onClick={() => upload.cancel()}>
                Cancel import
              </Button>
            ) : (
              <DialogClose asChild>
                <Button type="button" variant="outline">
                  Cancel
                </Button>
              </DialogClose>
            )}
            <Button type="submit" disabled={running || !file}>
              {running && <Spinner data-icon="inline-start" />}
              Import
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
