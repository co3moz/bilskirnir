import * as React from "react"
import {
  RiCornerDownRightLine,
  RiDownloadLine,
  RiHammerLine,
  RiImportLine,
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
import { attempt, runJob } from "@/lib/actions"
import { openDialog } from "@/lib/dialogs"
import { imageTree, shortDate } from "@/lib/format"
import { useImages, useProfiles } from "@/lib/queries"
import type { ImageRecord, ProfileItem } from "@/lib/types"

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
        <div
          className="flex min-w-48 items-start gap-2"
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
      <Badge variant="outline" className="border-violet/40 text-violet capitalize">
        {row.original.kind}
      </Badge>
    ),
  }),
  column.display({
    id: "desktop",
    header: "Desktop",
    cell: ({ row }) => (
      <span className="text-xs text-muted-foreground">
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
      const unavailable = row.original.available === false
      return (
        <div className="flex justify-end">
          <Button
            variant="outline"
            size="sm"
            disabled={unavailable}
            title={unavailable ? "Image no longer available" : undefined}
            onClick={() => openDialog({ kind: "create", image: row.original.image_id })}
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
              : "No saved images. Snapshot an environment or build an image."
          }
        />
        <DataTablePagination table={table} />
      </div>
    </PageBody>
  )
}
