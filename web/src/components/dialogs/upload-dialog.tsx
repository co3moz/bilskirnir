import * as React from "react"
import { RiUploadLine } from "@remixicon/react"
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
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { useFileDrop } from "@/hooks/use-file-drop"
import { sessionExpired } from "@/lib/api"
import { subfolderError, uploadLocations } from "@/lib/format"
import { useContainers, useSettings } from "@/lib/queries"
import { formatNumber, t } from "@/lib/i18n"
import { FileUploads, fileSize, uploadStatus } from "@/lib/uploads"
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
  const locations = item ? uploadLocations(item.spec) : []
  const [destination, setDestination] = React.useState(locations[0]?.target ?? "")
  const [subfolder, setSubfolder] = React.useState("")
  const [replace, setReplace] = React.useState(false)
  const [error, setError] = React.useState("")
  const input = React.useRef<HTMLInputElement>(null)
  const field = React.useId()

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

  const start = () => {
    const folder = subfolder.trim()
    const problem = subfolderError(folder)
    setError(problem)
    if (problem) return
    void uploads.start({
      target: `containers/${encodeURIComponent(id)}`,
      destination: folder ? `${destination}/${folder}` : destination,
      replace,
    })
  }

  const running = uploads.running
  const items = uploads.items
  const completed = items.filter((entry) => entry.status === "completed").length
  const unfinished = items.some((entry) => entry.status !== "completed")
  const retry = items.some((entry) => entry.status === "failed" || entry.status === "cancelled")

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("uploads.title")}</DialogTitle>
          <DialogDescription>{item?.spec.name}</DialogDescription>
        </DialogHeader>
        {!item || item.state !== "running" ? (
          <p className="text-sm text-muted-foreground">{t("uploads.notRunning")}</p>
        ) : !locations.length ? (
          <p className="text-sm text-muted-foreground">{t("uploads.noDestination")}</p>
        ) : (
          <FieldGroup className="gap-4">
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <Field>
                <FieldLabel htmlFor={`${field}-destination`}>{t("uploads.destination")}</FieldLabel>
                <Select value={destination} onValueChange={setDestination} disabled={running}>
                  <SelectTrigger id={`${field}-destination`} className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {locations.map((location) => (
                        <SelectItem key={location.target} value={location.target}>
                          {location.name}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </Field>
              <Field>
                <FieldLabel htmlFor={`${field}-subfolder`}>{t("uploads.subfolder")}</FieldLabel>
                <Input
                  id={`${field}-subfolder`}
                  placeholder={t("common.optional")}
                  autoComplete="off"
                  disabled={running}
                  value={subfolder}
                  onChange={(event) => setSubfolder(event.target.value)}
                />
              </Field>
            </div>
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
        <DialogFooter>
          {running && (
            <Button variant="outline" onClick={() => uploads.cancel()}>
              {t("uploads.cancel")}
            </Button>
          )}
          <Button disabled={running || !unfinished || !locations.length} onClick={start}>
            {running ? t("uploads.uploading") : retry ? t("uploads.retry") : t("uploads.title")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
