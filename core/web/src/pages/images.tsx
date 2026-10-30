import * as React from "react"
import {
  RiArchive2Line,
  RiCloudLine,
  RiCornerDownRightLine,
  RiDeleteBinLine,
  RiDownloadLine,
  RiFileCopyLine,
  RiHammerLine,
  RiImportLine,
  RiListSettingsLine,
  RiStackLine,
  RiAddLine,
  RiRefreshLine,
} from "@remixicon/react"

import { cn } from "cn"

import {
  columnHelper,
  DataTablePagination,
  DataTableView,
  ExtraRow,
  useDataTable,
  type DataColumn,
  type DataTable,
} from "@/components/data-table"
import { ConceptBadge, ConceptIcon } from "@/components/concept"
import { ImageIcon } from "@/components/image-icon"
import { DockerAlert, ListFailed, listFailed, PageBody, SectionHeading } from "@/components/page"
import { JobLogButton, PendingName, PendingStatus } from "@/components/pending"
import { StatusBadge } from "@/components/status-badge"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { attempt, exportArchive, exportBuildConfig, runJob } from "@/lib/actions"
import { composeEntries } from "@/lib/compose"
import { openDialog } from "@/lib/dialogs"
import {
  deletableImage,
  imageKind,
  imageSource,
  imageStatus,
  imageTree,
  shortDate,
} from "@/lib/format"
import { formatList, t } from "@/lib/i18n"
import { iconUrl } from "@/lib/icons"
import { catalogImageJobs, pendingImages, type Pending } from "@/lib/pending"
import { useImages, useJobs } from "@/lib/queries"
import type { ImageItem, Job } from "@/lib/types"
import { fileSize } from "@/lib/uploads"

/** What a compose asks for, in a few words. */
function composeSummary(compose: NonNullable<ImageItem["compose"]>) {
  const names = composeEntries(compose).map((entry) => entry.key)
  return names.length
    ? t("images.compose.asksFor", { names: formatList(names) })
    : t("images.compose.script")
}

/** Opens the delete dialog: a saved image goes with its record, a catalog image loses its download. */
function DeleteAction({ image }: { image: ImageItem }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="outline"
          size="icon-sm"
          className="text-destructive hover:text-destructive"
          onClick={() => openDialog({ kind: "image-delete", id: image.id })}
          aria-label={t("images.deleteOf", { name: image.name })}
        >
          <RiDeleteBinLine />
        </Button>
      </TooltipTrigger>
      <TooltipContent>
        {image.kind === "catalog" ? t("images.deleteDownload") : t("images.delete")}
      </TooltipContent>
    </Tooltip>
  )
}

/** Fetches a catalog image ahead of its first use, a newer release of it, or builds it here. */
function CatalogActions({ image }: { image: ImageItem }) {
  const [busy, setBusy] = React.useState(false)
  const job = (action: "pull" | "build") => async () => {
    setBusy(true)
    await attempt(() => runJob(`images/${image.id}/${action}`, {}))
    setBusy(false)
  }
  const downloaded = !!image.downloaded
  return (
    <>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="outline"
            size="icon-sm"
            disabled={busy}
            onClick={job("pull")}
            aria-label={
              downloaded
                ? t("images.updateOf", { name: image.name })
                : t("images.downloadOf", { name: image.name })
            }
          >
            {downloaded ? <RiRefreshLine /> : <RiDownloadLine />}
          </Button>
        </TooltipTrigger>
        <TooltipContent>
          {downloaded ? t("images.updateHint") : t("images.downloadHint")}
        </TooltipContent>
      </Tooltip>
      {!downloaded && (
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="outline"
              size="icon-sm"
              disabled={busy}
              onClick={job("build")}
              aria-label={t("images.buildOf", { name: image.name })}
            >
              <RiHammerLine />
            </Button>
          </TooltipTrigger>
          <TooltipContent>{t("images.buildHint")}</TooltipContent>
        </Tooltip>
      )}
    </>
  )
}

/** Moves a saved image: its build configuration, its compose and the image itself. */
function SavedActions({ image }: { image: ImageItem }) {
  return (
    <>
      {/* Only a script build has a configuration; a snapshot's changes have no script. */}
      {image.build && (
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="outline"
              size="icon-sm"
              onClick={() => attempt(() => exportBuildConfig(image))}
              aria-label={t("images.exportConfigOf", { name: image.name })}
            >
              <RiFileCopyLine />
            </Button>
          </TooltipTrigger>
          <TooltipContent>{t("images.exportConfig")}</TooltipContent>
        </Tooltip>
      )}
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="outline"
            size="icon-sm"
            onClick={() => openDialog({ kind: "compose", id: image.id })}
            aria-label={
              image.compose
                ? t("images.compose.editOf", { name: image.name })
                : t("images.compose.addOf", { name: image.name })
            }
          >
            <RiListSettingsLine />
          </Button>
        </TooltipTrigger>
        <TooltipContent>
          {image.compose ? t("compose.edit") : t("images.compose.addHint")}
        </TooltipContent>
      </Tooltip>
      {/* Any saved image moves as an archive, the base desktop included. */}
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="outline"
            size="icon-sm"
            disabled={image.available === false}
            onClick={() => exportArchive(image)}
            aria-label={t("archives.exportOf", { name: image.name })}
          >
            <RiArchive2Line />
          </Button>
        </TooltipTrigger>
        <TooltipContent>{t("archives.export")}</TooltipContent>
      </Tooltip>
    </>
  )
}

/** An image in its parent order, with the job downloading or building it for a catalog image. */
type Row = ImageItem & { depth: number; job?: Job }
const column = columnHelper<Row>()

const columns: DataColumn<Row>[] = [
  column.accessor("name", {
    header: () => t("table.name"),
    cell: ({ row }) => {
      const image = row.original
      const Icon =
        image.depth > 0
          ? RiCornerDownRightLine
          : image.kind === "catalog"
            ? RiCloudLine
            : RiStackLine
      const icon = iconUrl(image)
      return (
        // Table cells do not wrap; a long reference does here so the row actions stay in view.
        <div
          className="flex min-w-64 items-start gap-2 whitespace-normal"
          style={{ paddingLeft: `${Math.min(image.depth, 8) * 1.25}rem` }}
        >
          {image.kind === "catalog" ? (
            <ConceptIcon concept="image" icon={Icon} className="mt-0.5 size-4 shrink-0" />
          ) : (
            // A saved image's icon is where its icon is changed.
            <Tooltip>
              <TooltipTrigger asChild>
                <button
                  type="button"
                  onClick={() => openDialog({ kind: "icon", id: image.id })}
                  aria-label={t("icons.changeOf", { name: image.name })}
                  className={cn(
                    "flex shrink-0 items-center justify-center rounded-md outline-none hover:bg-muted focus-visible:ring-[3px] focus-visible:ring-ring/50",
                    icon ? "size-9" : "mt-0.5 size-4"
                  )}
                >
                  <ImageIcon src={icon} fallback={Icon} className={icon ? "size-9" : "size-4"} />
                </button>
              </TooltipTrigger>
              <TooltipContent>{icon ? t("icons.change") : t("icons.add")}</TooltipContent>
            </Tooltip>
          )}
          <div className="flex flex-col gap-0.5">
            {/* Docker's sizes are there once it has the image, and so are its layers. */}
            {image.size != null ? (
              <Button
                variant="link"
                className="h-auto w-fit p-0 text-left font-medium whitespace-normal text-foreground"
                onClick={() => openDialog({ kind: "image", id: image.id })}
              >
                {image.name}
              </Button>
            ) : (
              <span className="font-medium">{image.name}</span>
            )}
            {image.description && (
              <span className="line-clamp-2 text-xs text-muted-foreground">
                {image.description}
              </span>
            )}
            <code className="font-mono text-[0.7rem] break-all text-muted-foreground">
              {image.reference}
            </code>
          </div>
        </div>
      )
    },
  }),
  column.accessor("kind", {
    header: () => t("images.source"),
    cell: ({ row }) => (
      <div className="flex flex-wrap gap-1">
        <ConceptBadge concept="image">{imageKind(row.original.kind)}</ConceptBadge>
        {row.original.compose && (
          <Badge variant="outline" title={composeSummary(row.original.compose)}>
            {t("compose.name")}
          </Badge>
        )}
        {row.original.base_compose_missing && (
          <Badge
            variant="outline"
            className="border-warning/40 text-warning"
            title={t("images.compose.baseMissingHint")}
          >
            {t("images.compose.baseMissing")}
          </Badge>
        )}
        {!!row.original.mcp_servers?.length && (
          <ConceptBadge
            concept="mcp"
            title={t("images.mcpServers", { names: formatList(row.original.mcp_servers) })}
          >
            MCP
          </ConceptBadge>
        )}
      </div>
    ),
  }),
  column.display({
    id: "status",
    header: () => t("table.status"),
    cell: ({ row }) => {
      if (row.original.job) return <PendingStatus job={row.original.job} />
      const status = imageStatus(row.original)
      return (
        <span title={row.original.error ?? undefined}>
          <StatusBadge state={status.key} tone={status.tone} />
        </span>
      )
    },
  }),
  column.display({
    id: "desktop",
    header: () => t("nav.desktop"),
    // A desktop title wraps when the row actions need the room.
    cell: ({ row }) => (
      <span className="block min-w-28 text-xs whitespace-normal text-muted-foreground">
        {row.original.desktop?.title ??
          (row.original.kind === "catalog" ? "" : t("common.unknown"))}
      </span>
    ),
  }),
  column.accessor("size", {
    header: () => t("images.size"),
    cell: ({ row }) => {
      const { size, layers_size, own_size } = row.original
      if (size == null) return null
      // Only an image that shares layers with another one has a part of its own worth naming.
      const shares = own_size != null && layers_size != null && own_size < layers_size
      return (
        <div className="flex flex-col text-xs whitespace-nowrap">
          <span className="tabular-nums">{fileSize(size)}</span>
          {shares && (
            <span className="text-muted-foreground tabular-nums" title={t("images.ownHint")}>
              {t("images.ownSize", { size: fileSize(own_size) })}
            </span>
          )}
        </div>
      )
    },
  }),
  column.accessor("created_at", {
    header: () => t("table.created"),
    cell: ({ row }) =>
      row.original.created_at !== null && (
        <span className="text-xs whitespace-nowrap text-muted-foreground">
          {shortDate(row.original.created_at)}
        </span>
      ),
  }),
  column.display({
    id: "actions",
    cell: ({ row }) => {
      const image = row.original
      const catalog = image.kind === "catalog"
      // A catalog image that is not downloaded is still a start: the container downloads it.
      const unusable = catalog ? !!image.error : image.available === false
      return (
        <div className="flex justify-end gap-2">
          {/* While it downloads or builds, its job is all a catalog image offers besides a start. */}
          {image.job ? (
            <JobLogButton id={image.job.id} />
          ) : (
            <>
              {catalog ? <CatalogActions image={image} /> : <SavedActions image={image} />}
              {deletableImage(image) && <DeleteAction image={image} />}
            </>
          )}
          <Button
            variant="outline"
            size="sm"
            disabled={unusable}
            title={unusable ? (image.error ?? t("images.gone")) : undefined}
            onClick={() => openDialog({ kind: "create", image: imageSource(image) })}
          >
            <RiAddLine data-icon="inline-start" />
            {t("job.create")}
          </Button>
        </div>
      )
    },
  }),
]

/** A saved image a job is still making, above the images; it has nothing to act on yet. */
function PendingImageRow({
  table,
  pending,
}: {
  table: DataTable<Row>
  pending: Pending & { kind: string }
}) {
  return (
    <ExtraRow
      table={table}
      cells={{
        name: (
          <div className="flex min-w-64 items-start gap-2 whitespace-normal">
            <ConceptIcon concept="image" className="mt-0.5 size-4 shrink-0" />
            <PendingName pending={pending} />
          </div>
        ),
        kind: <ConceptBadge concept="image">{imageKind(pending.kind)}</ConceptBadge>,
        status: <PendingStatus job={pending.job} />,
        actions: (
          <div className="flex justify-end">
            <JobLogButton id={pending.job.id} />
          </div>
        ),
      }}
    />
  )
}

/** Every image in its parent order, the catalog images first and the ones being made above. */
export function ImagesPage() {
  const images = useImages()
  const jobs = useJobs().data
  const data = React.useMemo(() => {
    const running = catalogImageJobs(jobs ?? [])
    return imageTree(images.data ?? []).map((image) =>
      image.kind === "catalog" ? { ...image, job: running.get(image.id) } : image
    )
  }, [images.data, jobs])
  const pending = pendingImages(jobs ?? [], images.data ?? [])
  const table = useDataTable({ data, columns, getRowId: (row) => row.id })

  return (
    <PageBody>
      <DockerAlert />
      <SectionHeading title={t("images.title")} description={t("images.description")}>
        <Button variant="outline" size="sm" onClick={() => openDialog({ kind: "import" })}>
          <RiImportLine data-icon="inline-start" />
          {t("job.image-import")}
        </Button>
        <Button size="sm" onClick={() => openDialog({ kind: "build" })}>
          <RiHammerLine data-icon="inline-start" />
          {t("images.build")}
        </Button>
      </SectionHeading>
      <div className="flex flex-col gap-4 overflow-auto px-4 lg:px-6">
        <DataTableView
          table={table}
          empty={
            images.isPending ? (
              t("images.loading")
            ) : listFailed(images) ? (
              <ListFailed query={images} row />
            ) : (
              t("images.none")
            )
          }
          extra={pending.map((entry) => (
            <PendingImageRow key={entry.job.id} table={table} pending={entry} />
          ))}
        />
        <DataTablePagination table={table} />
      </div>
    </PageBody>
  )
}
