import * as React from "react"
import {
  RiArchive2Line,
  RiDatabase2Line,
  RiAddLine,
  RiFileCopyLine,
  RiFolderOpenLine,
  RiImportLine,
} from "@remixicon/react"

import {
  columnHelper,
  DataTablePagination,
  DataTableView,
  SortHeader,
  useDataTable,
  type DataColumn,
} from "@/components/data-table"
import { DockerAlert, PageBody, SectionHeading } from "@/components/page"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { attempt, exportVolume } from "@/lib/actions"
import { openDialog } from "@/lib/dialogs"
import { rich, t } from "@/lib/i18n"
import { useContainers, useSettings, useVolumes } from "@/lib/queries"
import type { Volume } from "@/lib/types"

// The ownership label the manager puts on what it creates; see src/model.rs.
const LABEL = "io.wsl-desktop.managed"
const MANAGED = `${LABEL}=true`

type Row = Volume & {
  users: { id: string; name: string; target: string; readonly: boolean }[]
  /** Whether to mark the volume as one Bilskirnir created. */
  managed: boolean
}
const column = columnHelper<Row>()

const columns: DataColumn<Row>[] = [
  column.accessor("Name", {
    header: ({ column }) => <SortHeader column={column}>{t("table.name")}</SortHeader>,
    cell: ({ row }) => (
      <div className="flex items-center gap-2">
        <RiDatabase2Line className="size-4 shrink-0 text-primary" aria-hidden />
        {row.original.manager ? (
          <span className="font-mono text-xs">{row.original.Name}</span>
        ) : (
          <button
            type="button"
            className="font-mono text-xs hover:underline"
            onClick={() => openDialog({ kind: "volume-files", name: row.original.Name })}
          >
            {row.original.Name}
          </button>
        )}
        {row.original.manager && (
          <Badge variant="outline" className="text-muted-foreground">
            {t("volumes.manager")}
          </Badge>
        )}
        {row.original.managed && (
          <Badge variant="outline" className="text-muted-foreground">
            Bilskirnir
          </Badge>
        )}
      </div>
    ),
  }),
  column.display({
    id: "users",
    header: () => t("volumes.mountedIn"),
    cell: ({ row }) =>
      row.original.users.length ? (
        <ul className="flex flex-col gap-0.5 text-xs">
          {row.original.users.map((user) => (
            <li key={`${user.id}-${user.target}`}>
              <button
                type="button"
                className="hover:underline"
                onClick={() => openDialog({ kind: "details", id: user.id })}
              >
                {user.name}
              </button>{" "}
              <span className="text-muted-foreground">
                {rich(user.readonly ? "volumes.mountedAtReadOnly" : "volumes.mountedAt", {
                  target: <code className="font-mono">{user.target}</code>,
                })}
              </span>
            </li>
          ))}
        </ul>
      ) : (
        <span className="text-xs text-muted-foreground">{t("volumes.notMounted")}</span>
      ),
  }),
  column.accessor("Driver", { header: () => t("volumes.driver") }),
  column.accessor("Scope", { header: () => t("volumes.scope") }),
  column.display({
    id: "actions",
    cell: ({ row }) =>
      row.original.manager ? (
        <p className="ml-auto max-w-64 text-right text-xs whitespace-normal text-muted-foreground">
          {t("volumes.managerHint")}
        </p>
      ) : (
        <div className="flex justify-end gap-2">
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="outline"
                size="icon-sm"
                onClick={() => attempt(async () => exportVolume(row.original.Name))}
                aria-label={t("archives.exportOf", { name: row.original.Name })}
              >
                <RiArchive2Line />
              </Button>
            </TooltipTrigger>
            <TooltipContent>{t("archives.export")}</TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="outline"
                size="icon-sm"
                onClick={() => openDialog({ kind: "volume-copy", name: row.original.Name })}
                aria-label={t("volumes.copyOf", { name: row.original.Name })}
              >
                <RiFileCopyLine />
              </Button>
            </TooltipTrigger>
            <TooltipContent>{t("volumes.copy")}</TooltipContent>
          </Tooltip>
          <Button
            variant="outline"
            size="sm"
            onClick={() => openDialog({ kind: "volume-files", name: row.original.Name })}
          >
            <RiFolderOpenLine data-icon="inline-start" />
            {t("volumes.browse")}
          </Button>
        </div>
      ),
  }),
]

export function VolumesPage() {
  const volumes = useVolumes()
  const containers = useContainers().data
  const settings = useSettings().data
  const filter = settings?.volume_filter
  // A filter on the ownership label lists only Bilskirnir's volumes, so marking them adds nothing.
  const marked = !!settings && filter?.split("=")[0] !== LABEL
  const data = React.useMemo<Row[]>(
    () =>
      (volumes.data ?? []).map((volume) => ({
        ...volume,
        users: (containers ?? []).flatMap((container) =>
          container.spec.mounts
            .filter((mount) => mount.volume === volume.Name)
            .map((mount) => ({
              id: container.id,
              name: container.spec.name,
              target: mount.target,
              readonly: mount.readonly,
            }))
        ),
        managed: marked && !!volume.Labels?.split(",").includes(MANAGED),
      })),
    [volumes.data, containers, marked]
  )
  const table = useDataTable({
    data,
    columns,
    getRowId: (row) => row.Name,
    sorting: [{ id: "Name", desc: false }],
  })

  return (
    <PageBody>
      <DockerAlert />
      <SectionHeading
        title={t("volumes.title")}
        description={
          filter
            ? `${t("volumes.description")} ${t("volumes.filtered", { label: filter })}`
            : t("volumes.description")
        }
      >
        <Button variant="outline" size="sm" onClick={() => openDialog({ kind: "volume-import" })}>
          <RiImportLine data-icon="inline-start" />
          {t("job.volume-import")}
        </Button>
        <Button size="sm" onClick={() => openDialog({ kind: "volume" })}>
          <RiAddLine data-icon="inline-start" />
          {t("volumes.new")}
        </Button>
      </SectionHeading>
      <div className="flex flex-col gap-4 overflow-auto px-4 lg:px-6">
        <DataTableView
          table={table}
          empty={volumes.isPending ? t("volumes.loading") : t("volumes.none")}
        />
        <DataTablePagination table={table} />
      </div>
    </PageBody>
  )
}
