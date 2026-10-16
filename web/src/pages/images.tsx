import * as React from "react"
import {
  RiArchive2Line,
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
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { attempt, exportArchive, exportBuildConfig, runJob } from "@/lib/actions"
import { openDialog } from "@/lib/dialogs"
import { imageTree, shortDate } from "@/lib/format"
import { useImages, useProfiles } from "@/lib/queries"
import type { ImageRecord, ProfileItem } from "@/lib/types"

/** What a template asks for, in a few words. */
function templateSummary(template: NonNullable<ImageRecord["template"]>) {
  const names = template.parameters.map((parameter) => parameter.name)
  return names.length ? `Asks for ${names.join(", ")}` : "Runs a setup script"
}

function ProfileCard({ item }: { item: ProfileItem }) {
  const { profile } = item
  const [busy, setBusy] = React.useState(false)
  const job = (path: string) => async () => {
    setBusy(true)
    await attempt(() => runJob(path, {}))
    setBusy(false)
  }
  const pull = job(`profiles/${profile.id}/pull`)
  return (
    <Card className="@container/card border-t-2 border-t-primary/40">
      <CardHeader>
        <CardTitle>{profile.name}</CardTitle>
        <CardDescription>{profile.description}</CardDescription>
        <CardAction>
          <StatusBadge
            state={item.available ? "ready" : "not_ready"}
            tone={item.available ? "success" : item.error ? "danger" : "muted"}
          />
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <code className="font-mono text-xs break-all text-muted-foreground">
          {profile.reference}
        </code>
        {item.error && (
          <Alert variant="destructive">
            <AlertDescription>{item.error}</AlertDescription>
          </Alert>
        )}
      </CardContent>
      <CardFooter className="mt-auto gap-2">
        {item.available ? (
          <>
            <Button onClick={() => openDialog({ kind: "create", image: profile.reference })}>
              <RiAddLine data-icon="inline-start" />
              Create environment
            </Button>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button variant="outline" disabled={busy} onClick={pull}>
                  <RiRefreshLine data-icon="inline-start" />
                  Update
                </Button>
              </TooltipTrigger>
              <TooltipContent>Download the latest published image</TooltipContent>
            </Tooltip>
          </>
        ) : (
          <>
            <Button disabled={busy} onClick={pull}>
              <RiDownloadLine data-icon="inline-start" />
              Download
            </Button>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="outline"
                  disabled={busy}
                  onClick={job(`profiles/${profile.id}/build`)}
                >
                  <RiHammerLine data-icon="inline-start" />
                  Build
                </Button>
              </TooltipTrigger>
              <TooltipContent>Build from the bundled Docker context</TooltipContent>
            </Tooltip>
          </>
        )}
      </CardFooter>
    </Card>
  )
}

type Row = ImageRecord & { depth: number }
const column = columnHelper<Row>()

const columns: DataColumn<Row>[] = [
  column.accessor("name", {
    header: "Name",
    cell: ({ row }) => {
      const image = row.original
      return (
        // Table cells do not wrap; a long reference does here so the row actions stay in view.
        <div
          className="flex min-w-64 items-start gap-2 whitespace-normal"
          style={{ paddingLeft: `${Math.min(image.depth, 8) * 1.25}rem` }}
        >
          {image.depth > 0 ? (
            <RiCornerDownRightLine className="mt-0.5 size-4 shrink-0 text-violet" aria-hidden />
          ) : (
            <RiStackLine className="mt-0.5 size-4 shrink-0 text-violet" aria-hidden />
          )}
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
    header: "Kind",
    cell: ({ row }) => (
      <div className="flex flex-wrap gap-1">
        <Badge variant="outline" className="border-violet/40 text-violet capitalize">
          {row.original.kind}
        </Badge>
        {row.original.template && (
          <Badge variant="outline" title={templateSummary(row.original.template)}>
            Template
          </Badge>
        )}
      </div>
    ),
  }),
  column.display({
    id: "desktop",
    header: "Desktop",
    // A profile title wraps when the row actions need the room.
    cell: ({ row }) => (
      <span className="block min-w-28 text-xs whitespace-normal text-muted-foreground">
        {row.original.desktop?.title ?? "Unknown"}
      </span>
    ),
  }),
  column.accessor("created_at", {
    header: "Created",
    cell: ({ row }) => (
      <span className="text-xs whitespace-nowrap text-muted-foreground">
        {shortDate(row.original.created_at)}
      </span>
    ),
  }),
  column.display({
    id: "actions",
    cell: ({ row }) => {
      const image = row.original
      const unavailable = image.available === false
      return (
        <div className="flex justify-end gap-2">
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
                onClick={() => openDialog({ kind: "template", id: image.id })}
                aria-label={`${image.template ? "Edit" : "Add"} the template of ${image.name}`}
              >
                <RiListSettingsLine />
              </Button>
            </TooltipTrigger>
            <TooltipContent>
              {image.template ? "Edit template" : "Add a template: values asked for on creation"}
            </TooltipContent>
          </Tooltip>
          {/* Any saved image moves as an archive, the base desktop included. */}
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="outline"
                size="icon-sm"
                disabled={unavailable}
                onClick={() => exportArchive(image)}
                aria-label={`Export ${image.name} as an archive`}
              >
                <RiArchive2Line />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Export archive (.tar.gz)</TooltipContent>
          </Tooltip>
          <Button
            variant="outline"
            size="sm"
            disabled={unavailable}
            title={unavailable ? "Image no longer available" : undefined}
            onClick={() => openDialog({ kind: "create", image: image.image_id })}
          >
            <RiAddLine data-icon="inline-start" />
            Create environment
          </Button>
        </div>
      )
    },
  }),
]

/** Supplied desktop profiles, then snapshots and custom images in their parent order. */
export function ImagesPage() {
  const profiles = useProfiles()
  const images = useImages()
  const data = React.useMemo(() => imageTree(images.data ?? []), [images.data])
  const table = useDataTable({ data, columns, getRowId: (row) => row.id })

  return (
    <PageBody>
      <DockerAlert />
      <SectionHeading
        title="Desktop profiles"
        description="Published desktops supplied with Bilskirnir."
      />
      <div className="grid grid-cols-1 gap-4 px-4 lg:px-6 @3xl/main:grid-cols-2">
        {profiles.isPending
          ? [0, 1].map((index) => <Skeleton key={index} className="h-48 rounded-xl" />)
          : profiles.data?.map((item) => <ProfileCard key={item.profile.id} item={item} />)}
      </div>
      <SectionHeading
        title="Snapshots and custom images"
        description="A child image starts from its parent's files."
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
        <DataTableView
          table={table}
          empty={
            images.isPending
              ? "Loading images…"
              : "No saved images. Snapshot an environment, build an image or import an archive."
          }
        />
        <DataTablePagination table={table} />
      </div>
    </PageBody>
  )
}
