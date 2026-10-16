import * as React from "react"
import {
  RiArrowLeftLine,
  RiDeleteBinLine,
  RiDownloadLine,
  RiFileLine,
  RiFileTextLine,
  RiFolderAddLine,
  RiFolderLine,
  RiLinkM,
  RiMoreLine,
  RiPencilLine,
  RiUploadLine,
} from "@remixicon/react"
import { useQuery } from "@tanstack/react-query"
import { cn } from "cn"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { FieldError } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Progress } from "@/components/ui/progress"
import { Skeleton } from "@/components/ui/skeleton"
import { Textarea } from "@/components/ui/textarea"
import { useFileDrop } from "@/hooks/use-file-drop"
import { api, sessionExpired } from "@/lib/api"
import { shortDate } from "@/lib/format"
import { queryClient, useSettings } from "@/lib/queries"
import { FileUploads, fileSize, uploadStatuses } from "@/lib/uploads"
import {
  EDIT_LIMIT,
  entryNameError,
  joinPath,
  parentPath,
  pathCrumbs,
  sortEntries,
  volumeApi,
  volumeDownloadUrl,
  type VolumeEntry,
  type VolumeListing,
} from "@/lib/volume-files"
import type { DialogProps } from "./side-drawer"

type Editing = { path: string; text: string; saved: string }
type Pending =
  | { kind: "folder" }
  | { kind: "rename"; entry: VolumeEntry }
  | { kind: "delete"; entry: VolumeEntry }

const icons: Record<VolumeEntry["type"], React.ReactNode> = {
  directory: <RiFolderLine className="size-4 shrink-0 text-primary" aria-hidden />,
  file: <RiFileLine className="size-4 shrink-0 text-muted-foreground" aria-hidden />,
  symlink: <RiLinkM className="size-4 shrink-0 text-muted-foreground" aria-hidden />,
  special: <RiFileLine className="size-4 shrink-0 text-muted-foreground" aria-hidden />,
}

function download(volume: string, path: string) {
  const link = document.createElement("a")
  link.href = volumeDownloadUrl(volume, path)
  link.download = ""
  document.body.append(link)
  link.click()
  link.remove()
}

/** The upload queue of the open folder, rendered again whenever it reports progress. */
function useUploads(onDone: () => void) {
  const [, render] = React.useReducer((count: number) => count + 1, 0)
  const [uploads] = React.useState(
    () =>
      new FileUploads({
        createRequest: () => new XMLHttpRequest(),
        onChange: render,
        onUnauthorized: sessionExpired,
      })
  )
  React.useEffect(() => () => uploads.cancel(), [uploads])
  const running = React.useRef(false)
  React.useEffect(() => {
    if (running.current && !uploads.running) onDone()
    running.current = uploads.running
  })
  return uploads
}

/** A name for a new folder or a renamed entry, asked for inline above the list. */
function NameForm({
  label,
  initial,
  busy,
  onSubmit,
  onCancel,
}: {
  label: string
  initial: string
  busy: boolean
  onSubmit: (name: string) => void
  onCancel: () => void
}) {
  const [name, setName] = React.useState(initial)
  const [error, setError] = React.useState("")
  return (
    <form
      className="flex flex-col gap-1.5"
      onSubmit={(event) => {
        event.preventDefault()
        const problem = entryNameError(name.trim())
        setError(problem)
        if (!problem) onSubmit(name.trim())
      }}
    >
      <div className="flex gap-2">
        <Input
          aria-label={label}
          placeholder={label}
          value={name}
          autoFocus
          disabled={busy}
          onChange={(event) => setName(event.target.value)}
          className="font-mono"
        />
        <Button type="submit" disabled={busy}>
          Save
        </Button>
        <Button type="button" variant="outline" disabled={busy} onClick={onCancel}>
          Cancel
        </Button>
      </div>
      <FieldError>{error}</FieldError>
    </form>
  )
}

/** Browses a volume's files and changes them as the desktop user. */
export function VolumeBrowser({ name, open, onOpenChange }: DialogProps & { name: string }) {
  const base = volumeApi(name)
  const limit = useSettings().data?.upload_limit_bytes ?? 0
  const [path, setPath] = React.useState("/")
  const [editing, setEditing] = React.useState<Editing | null>(null)
  const [pending, setPending] = React.useState<Pending | null>(null)
  const [discarding, setDiscarding] = React.useState(false)
  const [busy, setBusy] = React.useState(false)
  const [error, setError] = React.useState("")
  const key = ["volumes", name, "files", path]
  const listing = useQuery({
    queryKey: key,
    queryFn: () => api<VolumeListing>(`${base}/files/list`, { path, limit: 2000 }),
    enabled: open,
    retry: false,
  })
  const refresh = () => queryClient.invalidateQueries({ queryKey: ["volumes", name, "files"] })
  const uploads = useUploads(() => void refresh())
  const input = React.useRef<HTMLInputElement>(null)
  // A closing dialog stays mounted for its animation, so stop the queue as soon as it closes.
  React.useEffect(() => {
    if (!open) uploads.cancel()
  }, [open, uploads])

  const upload = (files: File[]) => {
    try {
      uploads.select(files, limit)
      setError("")
      void uploads.start({ target: base, destination: path, replace: false })
    } catch (failure) {
      setError((failure as Error).message)
    }
  }
  const drop = useFileDrop(upload)

  const run = async (action: () => Promise<unknown>, success?: string) => {
    setBusy(true)
    setError("")
    try {
      await action()
      setPending(null)
      if (success) toast.success(success)
      await refresh()
    } catch (failure) {
      setError((failure as Error).message)
    } finally {
      setBusy(false)
    }
  }

  const openEntry = async (entry: VolumeEntry) => {
    const target = joinPath(path, entry.name)
    if (entry.type === "directory") {
      setPath(target)
      setPending(null)
      setError("")
      return
    }
    if (entry.type !== "file") return
    if (entry.size > EDIT_LIMIT) {
      toast.info(`${entry.name} is larger than ${fileSize(EDIT_LIMIT)}; downloading it instead.`)
      download(name, target)
      return
    }
    setBusy(true)
    setError("")
    try {
      const file = await api<{ data: string }>(`${base}/files/read`, {
        path: target,
        limit: EDIT_LIMIT,
      })
      setEditing({ path: target, text: file.data, saved: file.data })
    } catch (failure) {
      setError((failure as Error).message)
    } finally {
      setBusy(false)
    }
  }

  const save = () =>
    editing &&
    run(async () => {
      await api(`${base}/files/write`, { path: editing.path, data: editing.text, replace: true })
      setEditing({ ...editing, saved: editing.text })
    }, `Saved ${editing.path}.`)

  const entries = sortEntries(listing.data?.items ?? [])
  const changed = !!editing && editing.text !== editing.saved
  const queue = uploads.items

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            Files in <code className="font-mono">{name}</code>
          </DialogTitle>
          <DialogDescription>
            Files are read and written as the desktop user (UID 1001), as in a container.
          </DialogDescription>
        </DialogHeader>
        {editing ? (
          <div className="flex min-w-0 flex-col gap-3">
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="icon-sm"
                aria-label="Back to the folder"
                onClick={() => (changed ? setDiscarding(true) : setEditing(null))}
              >
                <RiArrowLeftLine />
              </Button>
              <code className="truncate font-mono text-sm">{editing.path}</code>
            </div>
            {discarding && (
              <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-destructive/40 p-3 text-sm">
                <span>Discard the unsaved changes?</span>
                <div className="flex gap-2">
                  <Button
                    variant="destructive"
                    size="sm"
                    onClick={() => {
                      setDiscarding(false)
                      setEditing(null)
                    }}
                  >
                    Discard
                  </Button>
                  <Button variant="outline" size="sm" onClick={() => setDiscarding(false)}>
                    Keep editing
                  </Button>
                </div>
              </div>
            )}
            <Textarea
              aria-label={`Contents of ${editing.path}`}
              value={editing.text}
              spellCheck={false}
              onChange={(event) => setEditing({ ...editing, text: event.target.value })}
              className="h-[min(28rem,55vh)] resize-none font-mono text-xs"
            />
            <FieldError>{error}</FieldError>
            <DialogFooter>
              <Button
                variant="outline"
                onClick={() => download(name, editing.path)}
                disabled={busy || changed}
              >
                <RiDownloadLine data-icon="inline-start" />
                Download
              </Button>
              <Button onClick={save} disabled={busy || !changed}>
                {busy ? "Saving…" : "Save"}
              </Button>
            </DialogFooter>
          </div>
        ) : (
          <div
            {...drop.props}
            className={cn(
              "flex min-w-0 flex-col gap-3 rounded-lg",
              drop.over && "ring-2 ring-primary ring-offset-4 ring-offset-background"
            )}
          >
            <div className="flex flex-wrap items-center justify-between gap-2">
              <nav aria-label="Folder" className="flex min-w-0 flex-wrap items-center text-sm">
                {pathCrumbs(path).map((crumb, index, all) => (
                  <React.Fragment key={crumb.path}>
                    {index > 1 && <span className="px-1 text-muted-foreground">/</span>}
                    <button
                      type="button"
                      className={cn(
                        "rounded px-1 font-mono hover:underline",
                        index === all.length - 1 && "font-medium"
                      )}
                      aria-current={index === all.length - 1 ? "location" : undefined}
                      onClick={() => {
                        setPath(crumb.path)
                        setPending(null)
                      }}
                    >
                      {index === 0 ? name : crumb.name}
                    </button>
                  </React.Fragment>
                ))}
              </nav>
              <div className="flex gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  disabled={busy || uploads.running}
                  onClick={() => setPending({ kind: "folder" })}
                >
                  <RiFolderAddLine data-icon="inline-start" />
                  New folder
                </Button>
                <Button size="sm" disabled={uploads.running} onClick={() => input.current?.click()}>
                  <RiUploadLine data-icon="inline-start" />
                  Upload
                </Button>
                <input
                  ref={input}
                  type="file"
                  multiple
                  hidden
                  onChange={(event) => {
                    upload([...(event.target.files ?? [])])
                    event.target.value = ""
                  }}
                />
              </div>
            </div>
            {pending?.kind === "folder" && (
              <NameForm
                label="Folder name"
                initial=""
                busy={busy}
                onCancel={() => setPending(null)}
                onSubmit={(folder) =>
                  run(
                    () => api(`${base}/files/mkdir`, { path: joinPath(path, folder) }),
                    `Created ${folder}.`
                  )
                }
              />
            )}
            {pending?.kind === "rename" && (
              <NameForm
                label={`New name for ${pending.entry.name}`}
                initial={pending.entry.name}
                busy={busy}
                onCancel={() => setPending(null)}
                onSubmit={(renamed) =>
                  run(
                    () =>
                      api(`${base}/files/move`, {
                        source: joinPath(path, pending.entry.name),
                        destination: joinPath(path, renamed),
                      }),
                    `Renamed ${pending.entry.name} to ${renamed}.`
                  )
                }
              />
            )}
            {pending?.kind === "delete" && (
              <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-destructive/40 p-3 text-sm">
                <span>
                  Delete <strong className="font-medium">{pending.entry.name}</strong>
                  {pending.entry.type === "directory" ? " and everything in it" : ""}?
                </span>
                <div className="flex gap-2">
                  <Button
                    variant="destructive"
                    size="sm"
                    disabled={busy}
                    onClick={() =>
                      run(
                        () =>
                          api(`${base}/files/delete`, {
                            path: joinPath(path, pending.entry.name),
                            recursive: pending.entry.type === "directory",
                          }),
                        `Deleted ${pending.entry.name}.`
                      )
                    }
                  >
                    Delete
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={busy}
                    onClick={() => setPending(null)}
                  >
                    Cancel
                  </Button>
                </div>
              </div>
            )}
            <FieldError>{error || (listing.error as Error | null)?.message}</FieldError>
            <div className="max-h-[min(26rem,50vh)] overflow-y-auto rounded-lg border">
              {listing.isPending ? (
                <div className="flex flex-col gap-2 p-3">
                  {[0, 1, 2].map((index) => (
                    <Skeleton key={index} className="h-6" />
                  ))}
                </div>
              ) : (
                <ul className="divide-y">
                  {path !== "/" && (
                    <li>
                      <button
                        type="button"
                        className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-muted/50"
                        onClick={() => setPath(parentPath(path))}
                      >
                        <RiFolderLine className="size-4 text-muted-foreground" aria-hidden />
                        <span className="font-mono">..</span>
                      </button>
                    </li>
                  )}
                  {entries.map((entry) => (
                    <li key={entry.name} className="flex items-center gap-2 pr-2">
                      <button
                        type="button"
                        disabled={busy || entry.type === "symlink" || entry.type === "special"}
                        className="flex min-w-0 flex-1 items-center gap-2 px-3 py-2 text-left text-sm hover:bg-muted/50 disabled:cursor-default disabled:hover:bg-transparent"
                        onClick={() => void openEntry(entry)}
                        title={
                          entry.type === "file" && entry.size <= EDIT_LIMIT
                            ? "Open as text"
                            : undefined
                        }
                      >
                        {icons[entry.type]}
                        <span className="truncate font-mono">{entry.name}</span>
                        {entry.type === "file" && entry.size <= EDIT_LIMIT && (
                          <RiFileTextLine
                            className="hidden size-3.5 shrink-0 text-muted-foreground sm:block"
                            aria-hidden
                          />
                        )}
                      </button>
                      <span className="hidden w-20 shrink-0 text-right text-xs text-muted-foreground sm:block">
                        {entry.type === "file" ? fileSize(entry.size) : ""}
                      </span>
                      <span className="hidden w-32 shrink-0 text-right text-xs text-muted-foreground md:block">
                        {shortDate(entry.modified_at)}
                      </span>
                      <DropdownMenu>
                        <DropdownMenuTrigger asChild>
                          <Button
                            variant="ghost"
                            size="icon-sm"
                            disabled={busy}
                            aria-label={`${entry.name} actions`}
                          >
                            <RiMoreLine />
                          </Button>
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end" className="w-40">
                          {entry.type === "file" && (
                            <DropdownMenuItem
                              onSelect={() => download(name, joinPath(path, entry.name))}
                            >
                              <RiDownloadLine />
                              Download
                            </DropdownMenuItem>
                          )}
                          <DropdownMenuItem onSelect={() => setPending({ kind: "rename", entry })}>
                            <RiPencilLine />
                            Rename
                          </DropdownMenuItem>
                          <DropdownMenuSeparator />
                          <DropdownMenuItem
                            variant="destructive"
                            onSelect={() => setPending({ kind: "delete", entry })}
                          >
                            <RiDeleteBinLine />
                            Delete
                          </DropdownMenuItem>
                        </DropdownMenuContent>
                      </DropdownMenu>
                    </li>
                  ))}
                  {!entries.length && (
                    <li className="px-3 py-8 text-center text-sm text-muted-foreground">
                      {path === "/" ? "This volume is empty." : "This folder is empty."} Drop files
                      here to upload them.
                    </li>
                  )}
                </ul>
              )}
            </div>
            {listing.data?.truncated && (
              <p className="text-xs text-muted-foreground">
                Showing {listing.data.items.length} of {listing.data.total} entries.
              </p>
            )}
            {queue.length > 0 && (
              <ul className="flex max-h-40 flex-col gap-2 overflow-y-auto">
                {queue.map((entry, index) => (
                  <li key={`${entry.name}-${index}`} className="flex flex-col gap-1">
                    <div className="flex items-baseline justify-between gap-2 text-xs">
                      <span className="truncate font-mono">{entry.name}</span>
                      <span className="shrink-0 text-muted-foreground">
                        {entry.error || `${fileSize(entry.size)} · ${uploadStatuses[entry.status]}`}
                      </span>
                    </div>
                    <Progress
                      value={(entry.loaded * 100) / Math.max(entry.size, 1)}
                      aria-label={`${entry.name} upload progress`}
                    />
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}
