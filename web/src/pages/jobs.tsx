import * as React from "react"

import {
  columnHelper,
  DataTablePagination,
  DataTableView,
  SortHeader,
  useDataTable,
  ViewTabs,
  type DataColumn,
} from "@/components/data-table"
import { PageBody } from "@/components/page"
import { StatusBadge } from "@/components/status-badge"
import { Button } from "@/components/ui/button"
import { Tabs, TabsContent } from "@/components/ui/tabs"
import { openDialog } from "@/lib/dialogs"
import {
  duration,
  jobActive,
  jobNames,
  jobProgress,
  jobTarget,
  jobTone,
  shortDate,
} from "@/lib/format"
import { useContainers, useImages, useJobs, useProfiles } from "@/lib/queries"
import type { Job } from "@/lib/types"

type Row = Job & { target: string }
const column = columnHelper<Row>()

const columns: DataColumn<Row>[] = [
  column.accessor((job) => jobNames[job.kind] ?? job.kind, {
    id: "action",
    header: "Action",
    cell: ({ row, getValue }) => (
      <Button
        variant="link"
        className="h-auto p-0 text-foreground"
        onClick={() => openDialog({ kind: "job", id: row.original.id })}
      >
        {getValue()}
      </Button>
    ),
  }),
  column.accessor("target", {
    header: "Target",
    cell: ({ getValue }) => (
      <span className="block max-w-56 truncate text-xs text-muted-foreground">
        {getValue() || "None"}
      </span>
    ),
  }),
  column.accessor("status", {
    header: "Status",
    cell: ({ row }) => {
      const job = row.original
      const percent = job.status === "running" ? jobProgress(job.progress)?.percent : undefined
      return (
        <div className="flex items-center gap-2">
          <StatusBadge state={job.status} tone={jobTone(job.status)} />
          {percent !== undefined && (
            <span className="text-xs text-muted-foreground">{percent}%</span>
          )}
        </div>
      )
    },
  }),
  column.accessor("created_at", {
    header: ({ column }) => <SortHeader column={column}>Started</SortHeader>,
    cell: ({ row }) => (
      <span className="text-xs whitespace-nowrap text-muted-foreground">
        {shortDate(row.original.created_at)}
      </span>
    ),
  }),
  column.display({
    id: "duration",
    header: "Duration",
    cell: ({ row }) => {
      const { created_at, finished_at } = row.original
      return (
        <span className="text-xs text-muted-foreground tabular-nums">
          {finished_at ? duration(finished_at - created_at) : "In progress"}
        </span>
      )
    },
  }),
  column.display({
    id: "actions",
    cell: ({ row }) => (
      <div className="flex justify-end">
        <Button
          variant="outline"
          size="sm"
          onClick={() => openDialog({ kind: "job", id: row.original.id })}
        >
          View log
        </Button>
      </div>
    ),
  }),
]

const views = {
  all: () => true,
  active: jobActive,
  failed: (job: Job) => job.status === "failed" || job.status === "interrupted",
} satisfies Record<string, (job: Job) => boolean>
type View = keyof typeof views

/** The latest 100 manager jobs, each with its log. */
export function JobsPage() {
  const jobs = useJobs()
  const containers = useContainers().data
  const profiles = useProfiles().data
  const images = useImages().data
  const [view, setView] = React.useState<View>("all")
  const all = React.useMemo<Row[]>(() => {
    const names = new Map(containers?.map((container) => [container.id, container.spec.name]))
    const profileNames = new Map(profiles?.map(({ profile }) => [profile.id, profile.name]))
    const imageNames = new Map(images?.map((image) => [image.id, image.name]))
    return (jobs.data ?? []).map((job) => ({
      ...job,
      target: jobTarget(job, names, profileNames, imageNames),
    }))
  }, [jobs.data, containers, profiles, images])
  const data = React.useMemo(() => all.filter(views[view]), [all, view])
  const table = useDataTable({
    data,
    columns,
    getRowId: (job) => job.id,
    sorting: [{ id: "created_at", desc: true }],
  })
  const change = (value: string) => {
    setView(value as View)
    table.setPageIndex(0)
  }

  return (
    <PageBody>
      <Tabs value={view} onValueChange={change} className="w-full flex-col justify-start gap-6">
        <div className="flex items-center justify-between px-4 lg:px-6">
          <ViewTabs
            value={view}
            onChange={change}
            views={[
              { value: "all", label: "All jobs" },
              { value: "active", label: "In progress", count: all.filter(views.active).length },
              { value: "failed", label: "Failed", count: all.filter(views.failed).length },
            ]}
          />
        </div>
        <TabsContent
          value={view}
          className="relative flex flex-col gap-4 overflow-auto px-4 lg:px-6"
        >
          <DataTableView
            table={table}
            empty={jobs.isPending ? "Loading jobs…" : "No jobs in this view."}
          />
          <DataTablePagination table={table} />
        </TabsContent>
      </Tabs>
    </PageBody>
  )
}
