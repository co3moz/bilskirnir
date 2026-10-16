import * as React from "react"
import { RiDatabase2Line, RiAddLine } from "@remixicon/react"

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
import { openDialog } from "@/lib/dialogs"
import { useContainers, useVolumes } from "@/lib/queries"
import type { Volume } from "@/lib/types"

// The ownership label the manager puts on what it creates; see src/model.rs.
const MANAGED = "io.wsl-desktop.managed=true"

type Row = Volume & { users: { id: string; name: string; target: string; readonly: boolean }[] }
const column = columnHelper<Row>()

const columns: DataColumn<Row>[] = [
  column.accessor("Name", {
    header: ({ column }) => <SortHeader column={column}>Name</SortHeader>,
    cell: ({ row }) => (
      <div className="flex items-center gap-2">
        <RiDatabase2Line className="size-4 shrink-0 text-primary" aria-hidden />
        <code className="font-mono text-xs">{row.original.Name}</code>
        {row.original.Labels?.split(",").includes(MANAGED) && (
          <Badge variant="outline" className="text-muted-foreground">
            Bilskirnir
          </Badge>
        )}
      </div>
    ),
  }),
  column.display({
    id: "users",
    header: "Mounted in",
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
                at <code className="font-mono">{user.target}</code>
                {user.readonly ? " (read-only)" : ""}
              </span>
            </li>
          ))}
        </ul>
      ) : (
        <span className="text-xs text-muted-foreground">Not mounted</span>
      ),
  }),
  column.accessor("Driver", { header: "Driver" }),
  column.accessor("Scope", { header: "Scope" }),
]

export function VolumesPage() {
  const volumes = useVolumes()
  const containers = useContainers().data
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
      })),
    [volumes.data, containers]
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
        title="Named volumes"
        description="Files that outlive containers. Volumes are excluded from snapshots."
      >
        <Button size="sm" onClick={() => openDialog({ kind: "volume" })}>
          <RiAddLine data-icon="inline-start" />
          New volume
        </Button>
      </SectionHeading>
      <div className="flex flex-col gap-4 overflow-auto px-4 lg:px-6">
        <DataTableView
          table={table}
          empty={volumes.isPending ? "Loading volumes…" : "No volumes."}
        />
        <DataTablePagination table={table} />
      </div>
    </PageBody>
  )
}
