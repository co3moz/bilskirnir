import * as React from "react"
import {
  RiArrowUpLine,
  RiBookmarkLine,
  RiComputerLine,
  RiDatabase2Line,
  RiDownloadLine,
  RiFileLine,
  RiFileTextLine,
  RiFolderAddLine,
  RiFolderLine,
  RiHistoryLine,
  RiHome4Line,
  RiLinkM,
} from "@remixicon/react"
import { useQuery } from "@tanstack/react-query"
import { cn } from "cn"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { DialogFooter } from "@/components/ui/dialog"
import { FieldError } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import { api } from "@/lib/api"
import {
  canonicalFolder,
  folderMissing,
  HOME,
  listingProblem,
  placeGroups,
  quickFolders,
  uploadProblem,
  type QuickFolder,
} from "@/lib/desktop-paths"
import { formatNumber, t } from "@/lib/i18n"
import { queryClient } from "@/lib/queries"
import type { ContainerItem } from "@/lib/types"
import { fileSize } from "@/lib/uploads"
import {
  joinPath,
  parentPath,
  pathCrumbs,
  sortEntries,
  type VolumeEntry,
  type VolumeListing,
} from "@/lib/volume-files"
import { NameForm } from "./name-form"

const entryIcons: Record<VolumeEntry["type"], React.ReactNode> = {
  directory: <RiFolderLine className="size-4 shrink-0 text-primary" aria-hidden />,
  file: <RiFileLine className="size-4 shrink-0" aria-hidden />,
  symlink: <RiLinkM className="size-4 shrink-0" aria-hidden />,
  special: <RiFileLine className="size-4 shrink-0" aria-hidden />,
}

const homeIcons: Record<string, React.ReactNode> = {
  [HOME]: <RiHome4Line aria-hidden />,
  [`${HOME}/Desktop`]: <RiComputerLine aria-hidden />,
  [`${HOME}/Documents`]: <RiFileTextLine aria-hidden />,
  [`${HOME}/Downloads`]: <RiDownloadLine aria-hidden />,
}

const kindIcons: Record<Exclude<QuickFolder["kind"], "home">, React.ReactNode> = {
  compose: <RiBookmarkLine aria-hidden />,
  volume: <RiDatabase2Line aria-hidden />,
  recent: <RiHistoryLine aria-hidden />,
}

type Purpose =
  | { onChoose: (path: string) => void; onDownload?: undefined }
  /** `entry` is the file or folder in the list, or none for the open folder. */
  | { onDownload: (path: string, entry?: VolumeEntry) => void; onChoose?: undefined }

/**
 * Browses a running container's folders as the desktop user: the places beside the list and an
 * address that can be typed. With `onChoose` it picks a folder for an upload, offering a new
 * folder where the current one allows it; folders the manager would refuse can be opened but not
 * chosen. With `onDownload` it offers every file and folder, and the open folder, for download.
 */
export function FolderBrowser({
  container,
  initial,
  recent,
  onChoose,
  onDownload,
  onCancel,
}: {
  container: ContainerItem
  initial: string
  recent: string[]
  onCancel: () => void
} & Purpose) {
  const start = canonicalFolder(initial) ? initial : HOME
  const [path, setPath] = React.useState(start)
  const [address, setAddress] = React.useState(start)
  const [creating, setCreating] = React.useState(false)
  const [busy, setBusy] = React.useState(false)
  const [error, setError] = React.useState("")
  const base = `containers/${encodeURIComponent(container.id)}`
  const key = ["containers", container.id, "files", path]
  const listing = useQuery({
    queryKey: key,
    queryFn: () => api<VolumeListing>(`${base}/files/list`, { path, limit: 2000 }),
    retry: false,
    // The list says why it is empty, so the failure needs no toast.
    meta: { quiet: true },
  })
  const places = quickFolders(container, recent)
  const failed = (listing.error as Error | null)?.message ?? ""
  const failure = !failed
    ? ""
    : onDownload && folderMissing(failed)
      ? t("folders.notFound")
      : listingProblem(failed)
  // A folder that is missing is created by the upload; a file or a symlink on the path is not.
  const problem = onDownload
    ? failure
    : uploadProblem(container, path) || (failed && !folderMissing(failed) ? failure : "")

  const open = (target: string) => {
    if (!canonicalFolder(target)) {
      setError(t("folders.notCanonical"))
      return
    }
    setPath(target)
    setAddress(target)
    setCreating(false)
    setError("")
  }

  const create = async (name: string) => {
    const target = joinPath(path, name)
    setBusy(true)
    setError("")
    try {
      await api(`${base}/files/mkdir`, { path: target })
      toast.success(t("files.created", { name }))
      await queryClient.invalidateQueries({ queryKey: key })
      open(target)
    } catch (refusal) {
      setError((refusal as Error).message)
    } finally {
      setBusy(false)
    }
  }

  const entries = sortEntries(listing.data?.items ?? [])

  return (
    <div className="flex min-w-0 flex-col gap-3">
      <form
        className="flex min-w-0 items-center gap-2"
        onSubmit={(event) => {
          event.preventDefault()
          open(address.trim())
        }}
      >
        <Button
          type="button"
          variant="outline"
          size="icon-sm"
          aria-label={t("folders.up")}
          title={t("folders.up")}
          disabled={path === "/"}
          onClick={() => open(parentPath(path))}
        >
          <RiArrowUpLine />
        </Button>
        <Input
          aria-label={t("folders.address")}
          value={address}
          spellCheck={false}
          autoComplete="off"
          onChange={(event) => setAddress(event.target.value)}
          className="min-w-0 flex-1 font-mono"
        />
        <Button type="submit" variant="outline" size="sm">
          {t("folders.open")}
        </Button>
      </form>
      <div className="grid min-w-0 gap-3 sm:grid-cols-[minmax(0,11rem)_minmax(0,1fr)]">
        <nav
          aria-label={t("folders.places")}
          className="-mx-1 flex min-w-0 gap-1 overflow-x-auto px-1 pb-1 sm:mx-0 sm:flex-col sm:overflow-visible sm:p-0"
        >
          {placeGroups(places).map((group) => (
            <div
              key={group.places[0].path}
              className="flex shrink-0 gap-1 sm:flex-col sm:gap-0.5 sm:pt-3 sm:first:pt-0"
            >
              <span
                title={group.step}
                className="hidden truncate px-2 pb-0.5 text-xs text-muted-foreground sm:block"
              >
                {group.step ?? t(`folders.group.${group.kind}`)}
              </span>
              {group.places.map((place) => (
                <Button
                  key={place.path}
                  type="button"
                  variant={place.path === path ? "secondary" : "ghost"}
                  size="sm"
                  aria-current={place.path === path ? "location" : undefined}
                  title={place.path}
                  onClick={() => open(place.path)}
                  className="max-w-56 justify-start sm:max-w-none"
                >
                  {place.kind === "home" ? homeIcons[place.path] : kindIcons[place.kind]}
                  <span className={cn("truncate", place.kind === "recent" && "font-mono text-xs")}>
                    {place.label}
                  </span>
                </Button>
              ))}
            </div>
          ))}
        </nav>
        <div className="flex min-w-0 flex-col gap-2">
          <nav
            aria-label={t("files.folder")}
            className="flex min-w-0 flex-wrap items-center text-sm"
          >
            {pathCrumbs(path).map((crumb, index, all) => (
              <React.Fragment key={crumb.path}>
                {index > 1 && <span className="px-0.5 text-muted-foreground">/</span>}
                <button
                  type="button"
                  className={cn(
                    "rounded px-1 font-mono break-all hover:underline",
                    index === all.length - 1 && "font-medium"
                  )}
                  aria-current={index === all.length - 1 ? "location" : undefined}
                  onClick={() => open(crumb.path)}
                >
                  {crumb.name}
                </button>
              </React.Fragment>
            ))}
          </nav>
          {creating && (
            <NameForm
              label={t("files.folderName")}
              initial=""
              busy={busy}
              onCancel={() => setCreating(false)}
              onSubmit={(name) => void create(name)}
            />
          )}
          <div className="h-[min(20rem,40vh)] overflow-y-auto rounded-lg border">
            {listing.isPending ? (
              <div className="flex flex-col gap-2 p-3">
                {[0, 1, 2].map((index) => (
                  <Skeleton key={index} className="h-6" />
                ))}
              </div>
            ) : failure ? (
              <p className="px-3 py-8 text-center text-sm text-muted-foreground">{failure}</p>
            ) : (
              <ul className="divide-y">
                {entries.map((entry) =>
                  entry.type === "directory" ? (
                    <li key={entry.name} className="flex items-center">
                      <button
                        type="button"
                        className="flex min-w-0 flex-1 items-center gap-2 px-3 py-2 text-left text-sm hover:bg-muted/50"
                        onClick={() => open(joinPath(path, entry.name))}
                      >
                        {entryIcons.directory}
                        <span className="truncate font-mono">{entry.name}</span>
                      </button>
                      {onDownload && (
                        <Button
                          type="button"
                          variant="ghost"
                          size="icon-sm"
                          className="mr-1.5 shrink-0"
                          aria-label={t("downloads.archiveOf", { name: entry.name })}
                          title={t("downloads.archiveOf", { name: entry.name })}
                          onClick={() => onDownload(joinPath(path, entry.name), entry)}
                        >
                          <RiDownloadLine />
                        </Button>
                      )}
                    </li>
                  ) : onDownload && entry.type === "file" ? (
                    <li key={entry.name}>
                      <button
                        type="button"
                        className="flex w-full items-center gap-2 px-3 py-2 text-left text-sm hover:bg-muted/50"
                        aria-label={t("downloads.fileOf", { name: entry.name })}
                        title={t("downloads.fileOf", { name: entry.name })}
                        onClick={() => onDownload(joinPath(path, entry.name), entry)}
                      >
                        {entryIcons.file}
                        <span className="min-w-0 flex-1 truncate font-mono">{entry.name}</span>
                        <span className="shrink-0 text-xs text-muted-foreground">
                          {fileSize(entry.size)}
                        </span>
                        <RiDownloadLine
                          className="size-4 shrink-0 text-muted-foreground"
                          aria-hidden
                        />
                      </button>
                    </li>
                  ) : (
                    <li
                      key={entry.name}
                      className="flex items-center gap-2 px-3 py-2 text-sm text-muted-foreground"
                      title={entry.type === "symlink" ? t("folders.symlink") : undefined}
                    >
                      {entryIcons[entry.type]}
                      <span className="truncate font-mono">{entry.name}</span>
                    </li>
                  )
                )}
                {!entries.length && (
                  <li className="px-3 py-8 text-center text-sm text-muted-foreground">
                    {t("files.folderEmpty")}
                  </li>
                )}
              </ul>
            )}
          </div>
          {listing.data?.truncated && (
            <p className="text-xs text-muted-foreground">
              {t("files.truncated", {
                shown: formatNumber(listing.data.items.length),
                total: formatNumber(listing.data.total),
              })}
            </p>
          )}
        </div>
      </div>
      <FieldError>{error}</FieldError>
      <p role="status" aria-live="polite" className="text-xs text-muted-foreground">
        {problem || (onDownload ? t("downloads.hint") : t("folders.target", { path }))}
      </p>
      {onDownload ? (
        <DialogFooter>
          <Button type="button" variant="outline" onClick={onCancel}>
            {t("common.close")}
          </Button>
          <Button
            type="button"
            // The root is no folder of its own to archive; its folders are.
            disabled={path === "/" || !!problem || listing.isPending}
            onClick={() => onDownload(path)}
          >
            <RiDownloadLine data-icon="inline-start" />
            {t("downloads.folder")}
          </Button>
        </DialogFooter>
      ) : (
        <DialogFooter className="sm:justify-between">
          <Button
            type="button"
            variant="outline"
            disabled={busy || creating || !!listing.error || listing.isPending}
            onClick={() => setCreating(true)}
          >
            <RiFolderAddLine data-icon="inline-start" />
            {t("files.newFolder")}
          </Button>
          <div className="flex flex-col-reverse gap-2 sm:flex-row">
            <Button type="button" variant="outline" onClick={onCancel}>
              {t("common.cancel")}
            </Button>
            <Button type="button" disabled={!!problem} onClick={() => onChoose?.(path)}>
              {t("folders.choose")}
            </Button>
          </div>
        </DialogFooter>
      )}
    </div>
  )
}
