import * as React from "react"
import {
  RiArchive2Line,
  RiCloudLine,
  RiCornerDownRightLine,
  RiDownloadLine,
  RiFileCopyLine,
  RiHammerLine,
  RiImportLine,
  RiListSettingsLine,
  RiStackLine,
  RiAddLine,
  RiRefreshLine,
} from "@remixicon/react"

import {
  columnHelper,
  DataTablePagination,
  DataTableView,
  useDataTable,
  type DataColumn,
} from "@/components/data-table"
import { DockerAlert, PageBody, SectionHeading } from "@/components/page"
import { StatusBadge } from "@/components/status-badge"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { attempt, exportArchive, exportBuildConfig, runJob } from "@/lib/actions"
import { openDialog } from "@/lib/dialogs"
import { imageKinds, imageSource, imageStatus, imageTree, shortDate } from "@/lib/format"
import { useImages } from "@/lib/queries"
import type { ImageItem } from "@/lib/types"

/** What a compose asks for, in a few words. */
function composeSummary(compose: NonNullable<ImageItem["compose"]>) {
  const names = compose.parameters.map((parameter) => parameter.name)
  return names.length ? `Asks for ${names.join(", ")}` : "Runs a setup script"
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
            aria-label={`${downloaded ? "Update" : "Download"} ${image.name}`}
          >
            {downloaded ? <RiRefreshLine /> : <RiDownloadLine />}
          </Button>
        </TooltipTrigger>
        <TooltipContent>
          {downloaded ? "Download the latest published release" : "Download it now"}
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
              aria-label={`Build ${image.name} from source`}
            >
              <RiHammerLine />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Build from the bundled Docker context</TooltipContent>
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
              aria-label={`Export the configuration of ${image.name}`}
            >
              <RiFileCopyLine />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Export configuration as JSON</TooltipContent>
        </Tooltip>
      )}
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="outline"
            size="icon-sm"
            onClick={() => openDialog({ kind: "compose", id: image.id })}
            aria-label={`${image.compose ? "Edit" : "Add"} the compose of ${image.name}`}
          >
            <RiListSettingsLine />
          </Button>
        </TooltipTrigger>
        <TooltipContent>
          {image.compose ? "Edit compose" : "Add a compose: values asked for on creation"}
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
            aria-label={`Export ${image.name} as an archive`}
          >
            <RiArchive2Line />
          </Button>
        </TooltipTrigger>
        <TooltipContent>Export archive (.tar.gz)</TooltipContent>
      </Tooltip>
    </>
  )
}

type Row = ImageItem & { depth: number }
const column = columnHelper<Row>()

const columns: DataColumn<Row>[] = [
  column.accessor("name", {
    header: "Name",
    cell: ({ row }) => {
      const image = row.original
      const Icon =
        image.depth > 0
          ? RiCornerDownRightLine
          : image.kind === "catalog"
            ? RiCloudLine
            : RiStackLine
      return (
        // Table cells do not wrap; a long reference does here so the row actions stay in view.
        <div
          className="flex min-w-64 items-start gap-2 whitespace-normal"
          style={{ paddingLeft: `${Math.min(image.depth, 8) * 1.25}rem` }}
        >
          <Icon className="mt-0.5 size-4 shrink-0 text-violet" aria-hidden />
          <div className="flex flex-col gap-0.5">
            <span className="font-medium">{image.name}</span>
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
    header: "Source",
    cell: ({ row }) => (
      <div className="flex flex-wrap gap-1">
        <Badge variant="outline" className="border-violet/40 text-violet">
          {imageKinds[row.original.kind] ?? row.original.kind}
        </Badge>
        {row.original.compose && (
          <Badge variant="outline" title={composeSummary(row.original.compose)}>
            Compose
          </Badge>
        )}
      </div>
    ),
  }),
  column.display({
    id: "status",
    header: "Status",
    cell: ({ row }) => {
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
    header: "Desktop",
    // A desktop title wraps when the row actions need the room.
    cell: ({ row }) => (
      <span className="block min-w-28 text-xs whitespace-normal text-muted-foreground">
        {row.original.desktop?.title ?? (row.original.kind === "catalog" ? "" : "Unknown")}
      </span>
    ),
  }),
  column.accessor("created_at", {
    header: "Created",
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
          {catalog ? <CatalogActions image={image} /> : <SavedActions image={image} />}
          <Button
            variant="outline"
            size="sm"
            disabled={unusable}
            title={unusable ? (image.error ?? "Image no longer available") : undefined}
            onClick={() => openDialog({ kind: "create", image: imageSource(image) })}
          >
            <RiAddLine data-icon="inline-start" />
            Create container
          </Button>
        </div>
      )
    },
  }),
]

/** Every image in its parent order, the catalog images first. */
export function ImagesPage() {
  const images = useImages()
  const data = React.useMemo(() => imageTree(images.data ?? []), [images.data])
  const table = useDataTable({ data, columns, getRowId: (row) => row.id })

  return (
    <PageBody>
      <DockerAlert />
      <SectionHeading
        title="Image library"
        description="Catalog images download the first time you use them. A child image starts from its parent's files."
      >
        <Button variant="outline" size="sm" onClick={() => openDialog({ kind: "import" })}>
          <RiImportLine data-icon="inline-start" />
          Import image
        </Button>
        <Button size="sm" onClick={() => openDialog({ kind: "build" })}>
          <RiHammerLine data-icon="inline-start" />
          Build image
        </Button>
      </SectionHeading>
      <div className="flex flex-col gap-4 overflow-auto px-4 lg:px-6">
        <DataTableView table={table} empty={images.isPending ? "Loading images…" : "No images."} />
        <DataTablePagination table={table} />
      </div>
    </PageBody>
  )
}
