import * as React from "react"
import {
  RiPlayCircleLine,
  RiStopCircleLine,
  RiGamepadLine,
  RiMicLine,
  RiComputerLine,
  RiAddLine,
  RiVolumeUpLine,
  RiCloseLine,
} from "@remixicon/react"

import {
  ColumnsMenu,
  columnHelper,
  DataTablePagination,
  DataTableView,
  selectColumn,
  SortHeader,
  useDataTable,
  ViewTabs,
  type DataColumn,
} from "@/components/data-table"
import { InternetSwitch } from "@/components/container-card"
import { ContainerMenu } from "@/components/container-menu"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import { Tabs, TabsContent } from "@/components/ui/tabs"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { runJobs } from "@/lib/actions"
import { openDialog } from "@/lib/dialogs"
import { containerStatus, needsAttention, profileTitle, shortDate, stateLabels } from "@/lib/format"
import { useContainers, useProfiles } from "@/lib/queries"
import { href } from "@/lib/route"
import type { ContainerItem, ProfileItem } from "@/lib/types"

const column = columnHelper<ContainerItem>()

const views = {
  all: () => true,
  running: (item: ContainerItem) => item.state === "running",
  stopped: (item: ContainerItem) => item.state !== "running",
  attention: needsAttention,
} satisfies Record<string, (item: ContainerItem) => boolean>
type View = keyof typeof views

const labels: Record<string, string> = {
  status: "Status",
  resources: "Resources",
  devices: "Devices",
  internet: "Internet",
  created: "Created",
}

function columns(profiles: ProfileItem[]): DataColumn<ContainerItem>[] {
  return [
    selectColumn<ContainerItem>(),
    column.accessor((item) => item.spec.name, {
      id: "name",
      header: ({ column }) => <SortHeader column={column}>Name</SortHeader>,
      cell: ({ row }) => {
        const container = row.original
        return (
          <div className="flex min-w-40 flex-col">
            <Button
              variant="link"
              className="h-auto w-fit p-0 text-left text-foreground"
              onClick={() => openDialog({ kind: "details", id: container.id })}
            >
              {container.spec.name}
            </Button>
            <span className="text-xs text-muted-foreground">
              {profileTitle(container.spec, container.desktop?.title, profiles)}
            </span>
          </div>
        )
      },
      enableHiding: false,
    }),
    column.accessor((item) => stateLabels[containerStatus(item).key] ?? item.state, {
      id: "status",
      header: ({ column }) => <SortHeader column={column}>Status</SortHeader>,
      cell: ({ row }) => {
        const status = containerStatus(row.original)
        return <StatusBadge state={status.key} tone={status.tone} />
      },
    }),
    column.display({
      id: "resources",
      header: "Resources",
      cell: ({ row }) => {
        const { spec } = row.original
        return (
          <div className="flex flex-col text-xs">
            <span>
              {spec.gpu ? "GPU · " : ""}
              {spec.cpus ? `${spec.cpus} CPU` : "Unlimited CPU"}
            </span>
            <span className="text-muted-foreground">
              {spec.memory_mb ? `${spec.memory_mb} MB` : "Unlimited RAM"} · {spec.resolution}
            </span>
          </div>
        )
      },
    }),
    column.display({
      id: "devices",
      header: "Devices",
      cell: ({ row }) => {
        const { spec } = row.original
        const devices = [
          spec.audio && { icon: <RiVolumeUpLine />, label: "Audio" },
          spec.microphone && { icon: <RiMicLine />, label: "Microphone" },
          spec.gamepad && { icon: <RiGamepadLine />, label: "Gamepad" },
        ].filter((device) => !!device)
        if (!devices.length) return <span className="text-xs text-muted-foreground">None</span>
        return (
          <div className="flex gap-2 text-muted-foreground [&_svg]:size-4">
            {devices.map((device) => (
              <Tooltip key={device.label}>
                <TooltipTrigger asChild>
                  <span tabIndex={0} aria-label={device.label}>
                    {device.icon}
                  </span>
                </TooltipTrigger>
                <TooltipContent>{device.label}</TooltipContent>
              </Tooltip>
            ))}
          </div>
        )
      },
    }),
    column.display({
      id: "internet",
      header: "Internet",
      cell: ({ row }) => <InternetSwitch item={row.original} />,
    }),
    column.accessor((item) => item.created_at, {
      id: "created",
      header: ({ column }) => <SortHeader column={column}>Created</SortHeader>,
      cell: ({ row }) => (
        <span className="text-xs whitespace-nowrap text-muted-foreground">
          {shortDate(row.original.created_at)}
        </span>
      ),
    }),
    column.display({
      id: "actions",
      cell: ({ row }) => (
        <div className="flex items-center justify-end gap-1">
          {row.original.state === "running" && (
            <Tooltip>
              <TooltipTrigger asChild>
                <Button variant="ghost" size="icon" className="size-8" asChild>
                  <a href={href({ page: "desktop", id: row.original.id })}>
                    <RiComputerLine />
                    <span className="sr-only">Connect to {row.original.spec.name}</span>
                  </a>
                </Button>
              </TooltipTrigger>
              <TooltipContent>Connect</TooltipContent>
            </Tooltip>
          )}
          <ContainerMenu item={row.original} />
        </div>
      ),
    }),
  ]
}

/** dashboard-01's data table, holding the containers with bulk start and stop. */
export function ContainersTable() {
  const containers = useContainers()
  const profiles = useProfiles().data
  const [view, setView] = React.useState<View>("all")
  const all = React.useMemo(() => containers.data ?? [], [containers.data])
  const data = React.useMemo(() => all.filter(views[view]), [all, view])
  const cols = React.useMemo(() => columns(profiles ?? []), [profiles])
  const table = useDataTable({
    data,
    columns: cols,
    getRowId: (item) => item.id,
    selectable: true,
    sorting: [{ id: "name", desc: false }],
  })
  const selected = table.getSelectedRowModel().rows.map((row) => row.original)
  const startable = selected.filter((item) => item.state !== "running")
  const stoppable = selected.filter((item) => item.state === "running")
  const count = (key: View) => all.filter(views[key]).length

  const bulk = (items: ContainerItem[], action: "start" | "stop") => {
    table.resetRowSelection()
    void runJobs(
      items.map((item) => ({
        path: `containers/${item.id}/action`,
        body: { action },
      }))
    )
  }

  return (
    <Tabs
      value={view}
      onValueChange={(value) => {
        setView(value as View)
        table.setPageIndex(0)
      }}
      className="w-full flex-col justify-start gap-6"
    >
      <div className="flex flex-wrap items-center justify-between gap-2 px-4 lg:px-6">
        <ViewTabs
          value={view}
          onChange={(value) => {
            setView(value as View)
            table.setPageIndex(0)
          }}
          views={[
            { value: "all", label: "All containers" },
            { value: "running", label: "Running", count: count("running") },
            { value: "stopped", label: "Stopped", count: count("stopped") },
            { value: "attention", label: "Needs attention", count: count("attention") },
          ]}
        />
        <div className="flex items-center gap-2">
          {selected.length > 0 && (
            <>
              <Button
                variant="outline"
                size="sm"
                disabled={!startable.length}
                onClick={() => bulk(startable, "start")}
              >
                <RiPlayCircleLine data-icon="inline-start" />
                Start {startable.length || ""}
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={!stoppable.length}
                onClick={() => bulk(stoppable, "stop")}
              >
                <RiStopCircleLine data-icon="inline-start" />
                Stop {stoppable.length || ""}
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                onClick={() => table.resetRowSelection()}
                aria-label="Clear selection"
              >
                <RiCloseLine />
              </Button>
            </>
          )}
          <ColumnsMenu table={table} labels={labels} />
          <Button variant="outline" size="sm" onClick={() => openDialog({ kind: "create" })}>
            <RiAddLine data-icon="inline-start" />
            <span className="hidden lg:inline">New container</span>
          </Button>
        </div>
      </div>
      <TabsContent value={view} className="relative flex flex-col gap-4 overflow-auto px-4 lg:px-6">
        <DataTableView
          table={table}
          empty={
            containers.isPending
              ? "Loading containers…"
              : all.length
                ? "No containers in this view."
                : "No containers yet."
          }
        />
        <DataTablePagination table={table} selectable />
      </TabsContent>
    </Tabs>
  )
}
