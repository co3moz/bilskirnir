import * as React from "react"
import { RiFolderOpenLine, RiUploadLine } from "@remixicon/react"
import { cn } from "cn"

import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Progress } from "@/components/ui/progress"
import { useFileDrop } from "@/hooks/use-file-drop"
import { sessionExpired } from "@/lib/api"
import {
  defaultFolder,
  inVolume,
  recentFolders,
  rememberFolder,
  uploadProblem,
} from "@/lib/desktop-paths"
import { useContainers, useSettings } from "@/lib/queries"
import { formatNumber, t } from "@/lib/i18n"
import { FileUploads, fileSize, uploadStatus } from "@/lib/uploads"
import { FolderBrowser } from "./folder-browser"
import type { DialogProps } from "./side-drawer"

/** The upload queue, rendered again whenever it reports progress. */
function useUploads() {
  const [, render] = React.useReducer((count: number) => count + 1, 0)
  const [uploads] = React.useState(
    () =>
      new FileUploads({
        createRequest: () => new XMLHttpRequest(),
        onChange: render,
        onUnauthorized: sessionExpired,
      })
  )
  // Closing the dialog cancels what is left; completed files stay in the container.
  React.useEffect(() => () => uploads.cancel(), [uploads])
  return uploads
}

export function UploadDialog({
  id,
  files,
  open,
  onOpenChange,
}: DialogProps & { id: string; files?: File[] }) {
  const item = useContainers(false).data?.find((entry) => entry.id === id)
  const limit = useSettings().data?.upload_limit_bytes ?? 0
  const uploads = useUploads()
  // A closing dialog stays mounted for its animation, so stop the queue as soon as it closes.
  React.useEffect(() => {
    if (!open) uploads.cancel()
  }, [open, uploads])
  const [recent] = React.useState(() => recentFolders(id))
  const [destination, setDestination] = React.useState("")
  const [browsing, setBrowsing] = React.useState(false)
  const [replace, setReplace] = React.useState(false)
  const [checked, setChecked] = React.useState(false)
  const [error, setError] = React.useState("")
  const input = React.useRef<HTMLInputElement>(null)
  const field = React.useId()
  // The container record may arrive after the dialog opens; the folder is chosen once it has.
  const seeded = React.useRef(false)
  React.useEffect(() => {
    if (!item || seeded.current) return
    seeded.current = true
    setDestination(defaultFolder(item, recent))
  }, [item, recent])

  const select = React.useCallback(
    (chosen: File[]) => {
      try {
        uploads.select(chosen, limit)
        setError("")
      } catch (failure) {
        setError((failure as Error).message)
      }
    },
    [uploads, limit]
  )
  const initial = React.useRef(files)
  React.useEffect(() => {
    if (initial.current?.length) select(initial.current)
    initial.current = undefined
  }, [select])
  const drop = useFileDrop(select)

  const folder = destination.trim()
  const problem = item ? uploadProblem(item, folder) : ""

  const start = () => {
    setChecked(true)
    if (problem) return
    rememberFolder(id, folder)
    void uploads.start({
      target: `containers/${encodeURIComponent(id)}`,
      destination: folder,
      replace,
    })
  }

  const running = uploads.running
  const items = uploads.items
  const completed = items.filter((entry) => entry.status === "completed").length
  const unfinished = items.some((entry) => entry.status !== "completed")
  const retry = items.some((entry) => entry.status === "failed" || entry.status === "cancelled")
  const ready = item?.state === "running"

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className={browsing ? "sm:max-w-3xl" : "sm:max-w-lg"}>
        <DialogHeader>
          <DialogTitle>{browsing ? t("folders.title") : t("uploads.title")}</DialogTitle>
          <DialogDescription>{item?.spec.name}</DialogDescription>
        </DialogHeader>
        {!item || !ready ? (
          <p className="text-sm text-muted-foreground">{t("uploads.notRunning")}</p>
        ) : browsing ? (
          <FolderBrowser
            container={item}
            initial={folder}
            recent={recent}
            onCancel={() => setBrowsing(false)}
            onChoose={(path) => {
              setDestination(path)
              setChecked(true)
              setBrowsing(false)
            }}
          />
        ) : (
          <FieldGroup className="gap-4">
            <Field data-invalid={(checked && !!problem) || undefined}>
              <FieldLabel htmlFor={`${field}-destination`}>{t("uploads.destination")}</FieldLabel>
              <div className="flex gap-2">
                <Input
                  id={`${field}-destination`}
                  autoComplete="off"
                  spellCheck={false}
                  disabled={running}
                  value={destination}
                  aria-invalid={(checked && !!problem) || undefined}
                  onChange={(event) => setDestination(event.target.value)}
                  onBlur={() => setChecked(true)}
                  className="min-w-0 flex-1 font-mono"
                />
                <Button
                  type="button"
                  variant="outline"
                  disabled={running}
                  onClick={() => setBrowsing(true)}
                >
                  <RiFolderOpenLine data-icon="inline-start" />
                  {t("uploads.browse")}
                </Button>
              </div>
              {checked && problem ? (
                <FieldError>{problem}</FieldError>
              ) : (
                inVolume(item, folder) && (
                  <FieldDescription className="text-xs">{t("uploads.inVolume")}</FieldDescription>
                )
              )}
            </Field>
            <Field orientation="horizontal">
              <Checkbox
                id={`${field}-replace`}
                checked={replace}
                disabled={running}
                onCheckedChange={(value) => setReplace(!!value)}
              />
              <FieldLabel htmlFor={`${field}-replace`} className="font-normal">
                {t("uploads.replace")}
              </FieldLabel>
            </Field>
            <button
              type="button"
              {...drop.props}
              disabled={running}
              onClick={() => input.current?.click()}
              className={cn(
                "flex flex-col items-center justify-center gap-1 rounded-lg border border-dashed px-4 text-sm transition-colors outline-none hover:bg-muted/50 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50",
                items.length ? "py-4" : "py-10",
                drop.over && "border-primary bg-primary/5"
              )}
            >
              <RiUploadLine className="mb-1 size-5 text-muted-foreground" />
              <strong className="font-medium">{t("uploads.drop")}</strong>
              <span className="text-xs text-muted-foreground">
                {t("uploads.perFile", { size: fileSize(limit) })}
              </span>
            </button>
            <input
              ref={input}
              type="file"
              multiple
              hidden
              onChange={(event) => {
                select([...(event.target.files ?? [])])
                event.target.value = ""
              }}
            />
            {items.length > 0 && (
              <ul className="flex max-h-56 flex-col gap-3 overflow-y-auto">
                {items.map((entry, index) => (
                  <li key={`${entry.name}-${index}`} className="flex flex-col gap-1.5">
                    <div className="flex items-baseline justify-between gap-2 text-sm">
                      <strong className="truncate font-medium">{entry.name}</strong>
                      <span className="shrink-0 text-xs text-muted-foreground">
                        {fileSize(entry.size)} · {uploadStatus(entry.status)}
                      </span>
                    </div>
                    <Progress
                      value={(entry.loaded * 100) / Math.max(entry.size, 1)}
                      aria-label={t("uploads.progressOf", { name: entry.name })}
                    />
                    {entry.error && <p className="text-xs text-destructive">{entry.error}</p>}
                    {entry.path && (
                      <code className="font-mono text-xs break-all text-muted-foreground">
                        {entry.path}
                      </code>
                    )}
                  </li>
                ))}
              </ul>
            )}
            <p role="status" aria-live="polite" className="text-xs text-muted-foreground">
              {items.length
                ? t("uploads.done", {
                    completed: formatNumber(completed),
                    total: formatNumber(items.length),
                  })
                : ""}
            </p>
            <FieldDescription className="text-xs">{t("uploads.hint")}</FieldDescription>
            <FieldError>{error}</FieldError>
          </FieldGroup>
        )}
        {!browsing && (
          <DialogFooter>
            {running && (
              <Button variant="outline" onClick={() => uploads.cancel()}>
                {t("uploads.cancel")}
              </Button>
            )}
            <Button disabled={running || !unfinished || !ready} onClick={start}>
              {running ? t("uploads.uploading") : retry ? t("uploads.retry") : t("uploads.title")}
            </Button>
          </DialogFooter>
        )}
      </DialogContent>
    </Dialog>
  )
}
