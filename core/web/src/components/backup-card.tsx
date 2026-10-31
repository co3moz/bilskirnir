import * as React from "react"
import { RiAlertLine, RiDownloadLine, RiFileZipLine } from "@remixicon/react"
import { toast } from "sonner"
import { cn } from "cn"

import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Progress } from "@/components/ui/progress"
import { Spinner } from "@/components/ui/spinner"
import { useFileDrop } from "@/hooks/use-file-drop"
import { api } from "@/lib/api"
import {
  backupUrl,
  readable,
  readManifest,
  restoreUpload,
  restoreVersion,
  subscribeRestore,
  type BackupParts,
  type BackupReview,
} from "@/lib/backup"
import { openDialog } from "@/lib/dialogs"
import { plural, t, type MessageKey } from "@/lib/i18n"
import { refreshLists, useContainers, useImages, useSettings, useVolumes } from "@/lib/queries"
import { navigate } from "@/lib/route"
import { fileSize } from "@/lib/uploads"

type Part = Exclude<keyof BackupParts, "secrets">

/** How many of each part the installation has, as each choice says it. */
const hints = {
  images: { one: "backup.imagesHint.one", other: "backup.imagesHint.other" },
  files: { one: "backup.filesHint.one", other: "backup.filesHint.other" },
  volumes: { one: "backup.volumesHint.one", other: "backup.volumesHint.other" },
} as const satisfies Record<Part, { one: MessageKey; other: MessageKey }>

/** Lets the browser save the backup as the manager streams it, under the name it gives. */
function save(href: string) {
  const link = document.createElement("a")
  link.href = href
  link.download = ""
  document.body.append(link)
  link.click()
  link.remove()
}

function PartChoice({
  id,
  label,
  hint,
  checked,
  onChange,
}: {
  id: string
  label: string
  hint: string
  checked: boolean
  onChange: (checked: boolean) => void
}) {
  return (
    <Field orientation="horizontal" className="items-start">
      <Checkbox id={id} checked={checked} onCheckedChange={(next) => onChange(next === true)} />
      <div className="flex min-w-0 flex-col gap-1">
        <FieldLabel htmlFor={id}>{label}</FieldLabel>
        <FieldDescription>{hint}</FieldDescription>
      </div>
    </Field>
  )
}

/** What a backup carries, chosen before its download starts. */
function BackUp() {
  const containers = useContainers(false).data ?? []
  const images = (useImages(false).data ?? []).filter((image) => image.kind !== "catalog")
  const volumes = (useVolumes(false).data ?? []).filter((volume) => !volume.manager)
  const [parts, setParts] = React.useState<BackupParts>({
    images: true,
    files: true,
    volumes: true,
    secrets: false,
  })
  const id = React.useId()
  const set = (part: keyof BackupParts) => (checked: boolean) =>
    setParts((current) => ({ ...current, [part]: checked }))
  const counted = (part: Part, count: number) => plural(count, hints[part])

  const download = () => {
    save(backupUrl(parts))
    toast.success(t("backup.downloading"), {
      description: t("backup.downloadingDetail"),
      action: { label: t("nav.jobs"), onClick: () => navigate({ page: "jobs" }) },
    })
    setTimeout(() => void refreshLists("jobs"), 1000)
  }

  return (
    <section aria-labelledby={`${id}-title`} className="flex min-w-0 flex-col gap-4">
      <div className="flex flex-col gap-1">
        <h3 id={`${id}-title`} className="text-sm font-medium">
          {t("backup.create")}
        </h3>
        <p className="text-xs text-muted-foreground">{t("backup.always")}</p>
      </div>
      <FieldGroup className="gap-3">
        <PartChoice
          id={`${id}-images`}
          label={t("backup.images")}
          hint={counted("images", images.length)}
          checked={parts.images}
          onChange={set("images")}
        />
        <PartChoice
          id={`${id}-files`}
          label={t("backup.files")}
          hint={counted("files", containers.length)}
          checked={parts.files}
          onChange={set("files")}
        />
        <PartChoice
          id={`${id}-volumes`}
          label={t("backup.volumes")}
          hint={counted("volumes", volumes.length)}
          checked={parts.volumes}
          onChange={set("volumes")}
        />
        <PartChoice
          id={`${id}-secrets`}
          label={t("backup.secrets")}
          hint={t("backup.secretsHint")}
          checked={parts.secrets}
          onChange={set("secrets")}
        />
      </FieldGroup>
      {parts.secrets && (
        <Alert variant="destructive">
          <RiAlertLine />
          <AlertDescription>{t("backup.secretsWarning")}</AlertDescription>
        </Alert>
      )}
      <div className="flex flex-col items-start gap-2">
        <Button size="sm" onClick={download}>
          <RiDownloadLine data-icon="inline-start" />
          {t("backup.download")}
        </Button>
        <p className="text-xs text-muted-foreground">{t("backup.unencrypted")}</p>
      </div>
    </section>
  )
}

/** Chooses a backup, reads what it holds and asks before restoring it. */
function Restore() {
  const limit = useSettings().data?.archive_limit_bytes ?? 0
  React.useSyncExternalStore(subscribeRestore, restoreVersion)
  const upload = restoreUpload()
  const [reading, setReading] = React.useState(false)
  const [error, setError] = React.useState("")
  const input = React.useRef<HTMLInputElement>(null)
  const id = React.useId()

  const choose = async (files: File[]) => {
    const [file] = files
    if (!file || upload.running || reading) return
    if (files.length > 1) {
      setError(t("archives.single"))
      return
    }
    setError("")
    setReading(true)
    try {
      const manifest = await readManifest(file)
      if (!manifest) {
        setError(t("backup.notBackup"))
        return
      }
      if (!readable(manifest)) {
        setError(t("backup.newer", { version: manifest.version }))
        return
      }
      const review = await api<BackupReview>("backup/check", manifest)
      openDialog({ kind: "restore", file, manifest, review, limit })
    } catch (failure) {
      setError((failure as Error).message)
    } finally {
      setReading(false)
    }
  }
  const drop = useFileDrop((files) => void choose(files))

  const running = upload.running
  const status =
    upload.status === "checking"
      ? t("backup.finishing")
      : t("archives.uploaded", { current: fileSize(upload.loaded), total: fileSize(upload.size) })
  return (
    <section aria-labelledby={`${id}-title`} className="flex min-w-0 flex-col gap-4">
      <div className="flex flex-col gap-1">
        <h3 id={`${id}-title`} className="text-sm font-medium">
          {t("backup.restore")}
        </h3>
        <p className="text-xs text-muted-foreground">{t("backup.restoreHint")}</p>
      </div>
      <button
        type="button"
        {...drop.props}
        disabled={running || reading}
        onClick={() => input.current?.click()}
        className={cn(
          "flex flex-col items-center justify-center gap-1 rounded-lg border border-dashed px-4 py-6 text-sm transition-colors outline-none hover:bg-muted/50 focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50",
          drop.over && "border-primary bg-primary/5"
        )}
      >
        {reading ? (
          <Spinner className="mb-1 size-5 text-muted-foreground" />
        ) : (
          <RiFileZipLine className="mb-1 size-5 text-muted-foreground" />
        )}
        <strong className="font-medium">
          {reading ? t("backup.reading") : t("backup.choose")}
        </strong>
        <span className="text-xs text-muted-foreground">
          {t("archives.limit", { size: fileSize(limit) })}
        </span>
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
      {(running || upload.status === "cancelled") && upload.size > 0 && (
        <div role="status" aria-live="polite" className="flex flex-col gap-1.5">
          <Progress
            value={(upload.loaded * 100) / Math.max(upload.size, 1)}
            aria-label={t("archives.progress")}
          />
          <span className="text-xs text-muted-foreground">
            {upload.status === "cancelled" ? t("uploads.status.cancelled") : status}
          </span>
          {running && (
            <div className="flex flex-wrap items-center justify-between gap-2">
              <span className="text-xs text-muted-foreground">{t("backup.keepOpen")}</span>
              <Button type="button" variant="outline" size="sm" onClick={() => upload.cancel()}>
                {t("backup.cancel")}
              </Button>
            </div>
          )}
        </div>
      )}
      <FieldError>{error || (upload.status === "failed" ? upload.error : "")}</FieldError>
    </section>
  )
}

/**
 * The Settings page's backup of the whole installation beside its restore, one above the other on
 * a narrow page.
 */
export function BackupCard() {
  return (
    <Card>
      <CardContent className="grid grid-cols-1 gap-8 @3xl/main:grid-cols-2">
        <BackUp />
        <Restore />
      </CardContent>
    </Card>
  )
}
