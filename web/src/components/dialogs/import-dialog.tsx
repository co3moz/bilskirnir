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
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Textarea } from "@/components/ui/textarea"
import { useFileDrop } from "@/hooks/use-file-drop"
import { api, sessionExpired } from "@/lib/api"
import { ArchiveImport, archiveName, readArchiveInfo, type ArchiveInfo } from "@/lib/archives"
import { closeDialog, openDialog } from "@/lib/dialogs"
import { refreshAll, useSettings } from "@/lib/queries"
import { fileSize } from "@/lib/uploads"
import { useSubmit } from "./container-dialogs"
import { Footer } from "./simple-dialogs"
import type { DialogProps } from "./side-drawer"

type Tab = "archive" | "docker"

const descriptions: Record<Tab, string> = {
  archive: "Loads an archive exported from Bilskirnir, or saved with docker save, into Docker.",
  docker: "Registers a desktop-compatible image that is already on this Docker host.",
}

/** The upload, rendered again whenever it reports progress. */
function useArchiveImport() {
  const [, render] = React.useReducer((count: number) => count + 1, 0)
  const [upload] = React.useState(
    () =>
      new ArchiveImport({
        createRequest: () => new XMLHttpRequest(),
        onChange: render,
        onUnauthorized: sessionExpired,
      })
  )
  React.useEffect(() => () => upload.cancel(), [upload])
  return upload
}

function ArchiveForm({ open }: { open: boolean }) {
  const limit = useSettings().data?.archive_limit_bytes ?? 0
  const upload = useArchiveImport()
  // A closing dialog stays mounted for its animation, so stop the upload as soon as it closes.
  React.useEffect(() => {
    if (!open) upload.cancel()
  }, [open, upload])
  const [file, setFile] = React.useState<File | null>(null)
  const [info, setInfo] = React.useState<ArchiveInfo | null>(null)
  const [name, setName] = React.useState("")
  const [description, setDescription] = React.useState("")
  const [error, setError] = React.useState("")
  const input = React.useRef<HTMLInputElement>(null)
  const field = React.useId()

  const choose = async (files: File[]) => {
    const [chosen] = files
    if (!chosen || upload.running) return
    if (files.length > 1) {
      setError("Choose one archive at a time.")
      return
    }
    setFile(chosen)
    setError("")
    const found = await readArchiveInfo(chosen).catch(() => null)
    setInfo(found)
    setName(found?.name.trim().slice(0, 100) || archiveName(chosen.name))
    setDescription(found?.description ?? "")
  }
  const drop = useFileDrop((files) => void choose(files))

  const submit = async (event: React.FormEvent) => {
    event.preventDefault()
    if (!file) {
      setError("Choose an archive first.")
      return
    }
    setError("")
    const job = await upload.start(file, { name: name.trim(), description }, limit)
    if (!job) return
    closeDialog()
    toast.success(`Importing ${name.trim()}.`, {
      description: "The archive arrived. Docker is loading the image.",
    })
    void refreshAll()
    openDialog({ kind: "job", id: job })
  }

  const running = upload.running
  const status =
    upload.status === "checking"
      ? "Checking the archive…"
      : `${fileSize(upload.loaded)} of ${fileSize(upload.size)} uploaded`
  return (
    <form className="flex flex-col gap-4" onSubmit={submit}>
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
              <span className="text-xs text-muted-foreground">
                {fileSize(file.size)}
                {info?.desktop?.title ? ` · ${info.desktop.title}` : ""}
                {info?.kind ? ` · ${info.kind}` : ""}
              </span>
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
            void choose([...(event.target.files ?? [])])
            event.target.value = ""
          }}
        />
        <Field>
          <FieldLabel htmlFor={`${field}-name`}>Name</FieldLabel>
          <Input
            id={`${field}-name`}
            required
            maxLength={100}
            disabled={running}
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${field}-description`}>Description</FieldLabel>
          <Textarea
            id={`${field}-description`}
            rows={2}
            maxLength={4096}
            disabled={running}
            value={description}
            onChange={(event) => setDescription(event.target.value)}
          />
          <FieldDescription>
            Names and tags inside the archive are ignored; the image is saved under a new name.
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
          {running ? "Importing…" : "Import"}
        </Button>
      </DialogFooter>
    </form>
  )
}

function DockerImageForm() {
  const [name, setName] = React.useState("")
  const [reference, setReference] = React.useState("")
  const { busy, error, submit } = useSubmit()
  const field = React.useId()
  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={submit(async () => {
        await api("images", { name, reference })
        closeDialog()
        toast.success(`Imported ${name}.`)
        void refreshAll()
      })}
    >
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor={`${field}-name`}>Name</FieldLabel>
          <Input
            id={`${field}-name`}
            required
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${field}-reference`}>Image reference</FieldLabel>
          <Input
            id={`${field}-reference`}
            required
            placeholder="registry.example/team/desktop:1.0"
            value={reference}
            onChange={(event) => setReference(event.target.value)}
            className="font-mono"
          />
        </Field>
        <FieldError>{error}</FieldError>
      </FieldGroup>
      <Footer busy={busy} label="Import" />
    </form>
  )
}

/** Imports an image from an archive file or registers one already in Docker. */
export function ImportImageDialog(props: DialogProps) {
  const [tab, setTab] = React.useState<Tab>("archive")
  return (
    <Dialog {...props}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Import image</DialogTitle>
          <DialogDescription>{descriptions[tab]}</DialogDescription>
        </DialogHeader>
        <Tabs value={tab} onValueChange={(value) => setTab(value as Tab)} className="gap-4">
          <TabsList className="w-full">
            <TabsTrigger value="archive">Archive file</TabsTrigger>
            <TabsTrigger value="docker">Docker image</TabsTrigger>
          </TabsList>
          {/* Both stay mounted, so switching tabs keeps a running upload and the typed fields. */}
          <TabsContent value="archive" forceMount className="data-[state=inactive]:hidden">
            <ArchiveForm open={props.open} />
          </TabsContent>
          <TabsContent value="docker" forceMount className="data-[state=inactive]:hidden">
            <DockerImageForm />
          </TabsContent>
        </Tabs>
      </DialogContent>
    </Dialog>
  )
}
