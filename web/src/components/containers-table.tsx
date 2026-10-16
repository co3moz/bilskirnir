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
import { containerStatus, desktopTitle, needsAttention, shortDate, stateLabel } from "@/lib/format"
import { formatNumber, t } from "@/lib/i18n"
import { useContainers, useImages } from "@/lib/queries"
import { href } from "@/lib/route"
import type { ContainerItem, ImageItem } from "@/lib/types"

const column = columnHelper<ContainerItem>()

const views = {
  all: () => true,
  running: (item: ContainerItem) => item.state === "running",
  stopped: (item: ContainerItem) => item.state !== "running",
  attention: needsAttention,
} satisfies Record<string, (item: ContainerItem) => boolean>
type View = keyof typeof views

const columnLabels = (): Record<string, string> => ({
  status: t("table.status"),
  resources: t("table.resources"),
  devices: t("table.devices"),
  internet: t("spec.internet"),
  created: t("table.created"),
})

function columns(images: ImageItem[]): DataColumn<ContainerItem>[] {
  return [
    selectColumn<ContainerItem>(),
    column.accessor((item) => item.spec.name, {
      id: "name",
      header: ({ column }) => <SortHeader column={column}>{t("table.name")}</SortHeader>,
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
              {desktopTitle(container.spec, container.desktop?.title, images)}
            </span>
          </div>
        )
      },
      enableHiding: false,
    }),
    column.accessor((item) => stateLabel(containerStatus(item).key), {
      id: "status",
      header: ({ column }) => <SortHeader column={column}>{t("table.status")}</SortHeader>,
      cell: ({ row }) => {
        const status = containerStatus(row.original)
        return <StatusBadge state={status.key} tone={status.tone} />
      },
    }),
    column.display({
      id: "resources",
      header: () => t("table.resources"),
      cell: ({ row }) => {
        const { spec } = row.original
        return (
          <div className="flex flex-col text-xs">
            <span>
              {spec.gpu ? `${t("spec.gpu")} · ` : ""}
              {spec.cpus
                ? t("spec.cpus", { count: formatNumber(spec.cpus) })
                : t("spec.unlimitedCpu")}
            </span>
            <span className="text-muted-foreground">
              {spec.memory_mb
                ? t("spec.memory", { count: formatNumber(spec.memory_mb) })
                : t("spec.unlimitedMemory")}{" "}
              · {spec.resolution}
            </span>
          </div>
        )
      },
    }),
    column.display({
      id: "devices",
      header: () => t("table.devices"),
      cell: ({ row }) => {
        const { spec } = row.original
        const devices = [
          spec.audio && { icon: <RiVolumeUpLine />, label: t("spec.audio") },
          spec.microphone && { icon: <RiMicLine />, label: t("spec.microphone") },
          spec.gamepad && { icon: <RiGamepadLine />, label: t("spec.gamepad") },
        ].filter((device) => !!device)
        if (!devices.length)
          return <span className="text-xs text-muted-foreground">{t("common.none")}</span>
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
      header: () => t("spec.internet"),
      cell: ({ row }) => <InternetSwitch item={row.original} />,
    }),
    column.accessor((item) => item.created_at, {
      id: "created",
      header: ({ column }) => <SortHeader column={column}>{t("table.created")}</SortHeader>,
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
                    <span className="sr-only">
                      {t("containers.connectTo", { name: row.original.spec.name })}
                    </span>
                  </a>
                </Button>
              </TooltipTrigger>
              <TooltipContent>{t("containers.connect")}</TooltipContent>
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
  const images = useImages().data
  const [view, setView] = React.useState<View>("all")
  const all = React.useMemo(() => containers.data ?? [], [containers.data])
  const data = React.useMemo(() => all.filter(views[view]), [all, view])
  const cols = React.useMemo(() => columns(images ?? []), [images])
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
            { value: "all", label: t("containers.view.all") },
            { value: "running", label: t("state.running"), count: count("running") },
            { value: "stopped", label: t("state.exited"), count: count("stopped") },
            {
              value: "attention",
              label: t("containers.view.attention"),
              count: count("attention"),
            },
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
                {t("containers.start")} {startable.length || ""}
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={!stoppable.length}
                onClick={() => bulk(stoppable, "stop")}
              >
                <RiStopCircleLine data-icon="inline-start" />
                {t("containers.stop")} {stoppable.length || ""}
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                onClick={() => table.resetRowSelection()}
                aria-label={t("table.clearSelection")}
              >
                <RiCloseLine />
              </Button>
            </>
          )}
          <ColumnsMenu table={table} labels={columnLabels()} />
          <Button variant="outline" size="sm" onClick={() => openDialog({ kind: "create" })}>
            <RiAddLine data-icon="inline-start" />
            <span className="hidden lg:inline">{t("containers.new")}</span>
          </Button>
        </div>
      </div>
      <TabsContent value={view} className="relative flex flex-col gap-4 overflow-auto px-4 lg:px-6">
        <DataTableView
          table={table}
          empty={
            containers.isPending
              ? t("containers.loading")
              : all.length
                ? t("containers.noneInView")
                : t("containers.noneYet")
          }
        />
        <DataTablePagination table={table} selectable />
      </TabsContent>
    </Tabs>
  )
}
