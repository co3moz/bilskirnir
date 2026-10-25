import * as React from "react"
import { RiDownload2Line, RiNodeTree, RiSearchLine } from "@remixicon/react"
import type { Row } from "@tanstack/react-table"

import {
  columnHelper,
  DataTablePagination,
  DataTableView,
  SortHeader,
  useDataTable,
  ViewTabs,
  type DataColumn,
  type Features,
} from "@/components/data-table"
import { PageBody, SectionHeading } from "@/components/page"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import { Tabs, TabsContent } from "@/components/ui/tabs"
import { saveData } from "@/lib/actions"
import { ApiError } from "@/lib/api"
import {
  groupFilters,
  groups,
  isScope,
  keeps,
  listFileName,
  matches,
  shortCommit,
  type Dependency,
  type DependencyFilter,
  type DependencyGroup,
  type DependencyKind,
} from "@/lib/dependencies"
import { formatNumber, t } from "@/lib/i18n"
import { useDependencies } from "@/lib/queries"

const column = columnHelper<Dependency>()

/** Text in the reader's order, whatever its case: `Swatinem/rust-cache` among the other names in s. */
const byText = (a: Row<Features, Dependency>, b: Row<Features, Dependency>, id: string) =>
  String(a.getValue(id)).localeCompare(String(b.getValue(id)), undefined, { sensitivity: "base" })

function filterLabel(filter: DependencyFilter) {
  if (filter === "all") return t("common.all")
  if (filter === "direct") return t("dependencies.direct")
  if (filter.startsWith("license:")) return filter.slice("license:".length)
  if (isScope(filter)) return t(`dependencies.scope.${filter}`)
  return t(`dependencies.kind.${filter as DependencyKind}`)
}

const name: DataColumn<Dependency> = column.accessor("name", {
  sortFn: byText,
  header: ({ column }) => <SortHeader column={column}>{t("table.name")}</SortHeader>,
  cell: ({ row }) => {
    const item = row.original
    return (
      <div className="flex max-w-56 min-w-40 flex-col gap-0.5 @3xl/main:max-w-md">
        <div className="flex items-center gap-2">
          {item.url ? (
            <a
              href={item.url}
              target="_blank"
              rel="noreferrer"
              className="truncate font-mono text-xs hover:underline"
            >
              {item.name}
            </a>
          ) : (
            <span className="truncate font-mono text-xs">{item.name}</span>
          )}
          {item.direct && (
            <Badge variant="outline" className="text-muted-foreground">
              {t("dependencies.direct")}
            </Badge>
          )}
        </div>
        {item.description && (
          <span className="truncate text-xs text-muted-foreground" title={item.description}>
            {item.description}
          </span>
        )}
      </div>
    )
  },
})

const version: DataColumn<Dependency> = column.accessor((item) => item.version ?? "", {
  id: "version",
  header: () => t("dependencies.column.version"),
  cell: ({ row }) => {
    const item = row.original
    return (
      <div className="flex flex-col gap-0.5">
        {item.version ? (
          <span className="max-w-40 truncate font-mono text-xs" title={item.version}>
            {item.version}
          </span>
        ) : (
          <span className="text-xs text-muted-foreground">{t("dependencies.unpinned")}</span>
        )}
        {item.source && (
          <span className="font-mono text-xs text-muted-foreground">
            {t("dependencies.installedIn", { image: item.source })}
          </span>
        )}
        {item.pin && (
          <span className="font-mono text-xs text-muted-foreground" title={item.pin}>
            {t("dependencies.pinned", { commit: shortCommit(item.pin) })}
          </span>
        )}
      </div>
    )
  },
})

const license: DataColumn<Dependency> = column.accessor((item) => item.license ?? "", {
  id: "license",
  sortFn: byText,
  header: ({ column }) => (
    <SortHeader column={column}>{t("dependencies.column.license")}</SortHeader>
  ),
  cell: ({ row }) =>
    row.original.license ? (
      <span className="block max-w-56 truncate text-xs" title={row.original.license}>
        {row.original.license}
      </span>
    ) : (
      <span className="text-xs text-muted-foreground">{t("common.unknown")}</span>
    ),
})

const scope: DataColumn<Dependency> = column.accessor((item) => item.scope ?? "", {
  id: "scope",
  sortFn: byText,
  header: ({ column }) => <SortHeader column={column}>{t("dependencies.column.scope")}</SortHeader>,
  cell: ({ row }) =>
    row.original.scope && (
      <Badge variant="outline" className="text-muted-foreground">
        {t(`dependencies.scope.${row.original.scope}`)}
      </Badge>
    ),
})

const kind: DataColumn<Dependency> = column.accessor((item) => item.kind ?? "", {
  id: "kind",
  sortFn: byText,
  header: ({ column }) => <SortHeader column={column}>{t("dependencies.column.kind")}</SortHeader>,
  cell: ({ row }) =>
    row.original.kind && (
      <span className="text-xs">{t(`dependencies.kind.${row.original.kind}`)}</span>
    ),
})

const usedIn: DataColumn<Dependency> = column.display({
  id: "used_in",
  header: () => t("dependencies.column.usedIn"),
  cell: ({ row }) => (
    <div className="flex flex-wrap gap-1">
      {row.original.used_in?.map((place) => (
        <Badge key={place} variant="outline" className="text-muted-foreground">
          {t(`dependencies.place.${place}`)}
        </Badge>
      ))}
    </div>
  ),
})

const packageColumns: DataColumn<Dependency>[] = [name, version, license, scope]
const infrastructureColumns: DataColumn<Dependency>[] = [name, version, kind, usedIn]

const rowId = (item: Dependency) =>
  [item.group, item.kind, item.name, item.version, item.source].join("|")

function Loading() {
  return (
    <div
      className="flex flex-col gap-3 px-4 lg:px-6"
      role="status"
      aria-label={t("dependencies.loading")}
    >
      <Skeleton className="h-8 w-72" />
      <Skeleton className="h-96 rounded-lg" />
    </div>
  )
}

function Unavailable({ error }: { error: Error }) {
  const missing = error instanceof ApiError && error.status === 404
  return (
    <div className="px-4 lg:px-6">
      <Empty className="border">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <RiNodeTree />
          </EmptyMedia>
          <EmptyTitle>{missing ? t("dependencies.missing") : t("dependencies.failed")}</EmptyTitle>
          <EmptyDescription>
            {missing ? t("dependencies.missingHint") : error.message}
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    </div>
  )
}

/** Every interface package, crate and piece of infrastructure this manager was built from. */
export function DependenciesPage() {
  const query = useDependencies()
  const list = query.data
  const [group, setGroup] = React.useState<DependencyGroup>("ui")
  const [filter, setFilter] = React.useState<DependencyFilter>("all")
  const [search, setSearch] = React.useState("")
  const items = React.useMemo(
    () => (list?.dependencies ?? []).filter((item) => item.group === group),
    [list, group]
  )
  const shown = React.useMemo(
    () => items.filter((item) => keeps(item, filter) && matches(item, search)),
    [items, filter, search]
  )
  const { filters, licenses } = React.useMemo(() => groupFilters(items), [items])
  const table = useDataTable({
    data: shown,
    columns: group === "infrastructure" ? infrastructureColumns : packageColumns,
    getRowId: rowId,
    sorting: [{ id: "name", desc: false }],
  })
  const changeGroup = (value: string) => {
    setGroup(value as DependencyGroup)
    setFilter("all")
    table.setPageIndex(0)
  }
  const changeFilter = (value: string) => {
    setFilter(value as DependencyFilter)
    table.setPageIndex(0)
  }
  const changeSearch = (value: string) => {
    setSearch(value)
    table.setPageIndex(0)
  }

  if (query.error)
    return (
      <PageBody>
        <Unavailable error={query.error} />
      </PageBody>
    )
  if (!list)
    return (
      <PageBody>
        <Loading />
      </PageBody>
    )
  return (
    <PageBody>
      <SectionHeading
        title={t("dependencies.heading", { version: list.version })}
        description={t("dependencies.description")}
      >
        <Button
          variant="outline"
          size="sm"
          onClick={() =>
            saveData(listFileName(list), JSON.stringify(list, null, 2), "application/json")
          }
        >
          <RiDownload2Line data-icon="inline-start" />
          {t("dependencies.download")}
        </Button>
      </SectionHeading>
      <Tabs
        value={group}
        onValueChange={changeGroup}
        className="w-full flex-col justify-start gap-4"
      >
        <div className="flex flex-wrap items-center justify-between gap-2 px-4 lg:px-6">
          <ViewTabs
            value={group}
            onChange={changeGroup}
            views={groups.map((value) => ({ value, label: t(`dependencies.group.${value}`) }))}
          />
          <div className="flex flex-wrap items-center gap-2">
            <Select value={filter} onValueChange={changeFilter}>
              <SelectTrigger size="sm" className="w-48" aria-label={t("dependencies.filter")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  {filters.map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      {`${filterLabel(option.value)} (${formatNumber(option.count)})`}
                    </SelectItem>
                  ))}
                </SelectGroup>
                {licenses.length > 0 && (
                  <>
                    <SelectSeparator />
                    <SelectGroup>
                      <SelectLabel>{t("dependencies.column.license")}</SelectLabel>
                      {licenses.map((option) => (
                        <SelectItem key={option.value} value={option.value}>
                          {`${option.license} (${formatNumber(option.count)})`}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </>
                )}
              </SelectContent>
            </Select>
            <div className="relative">
              <RiSearchLine className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
              <Input
                type="search"
                aria-label={t("dependencies.search")}
                placeholder={t("dependencies.search")}
                value={search}
                onChange={(event) => changeSearch(event.target.value)}
                className="h-8 w-48 pl-8 sm:w-56"
              />
            </div>
          </div>
        </div>
        <TabsContent
          value={group}
          className="relative flex flex-col gap-4 overflow-auto px-4 lg:px-6"
        >
          <DataTableView table={table} empty={t("dependencies.none")} />
          <DataTablePagination table={table} />
        </TabsContent>
      </Tabs>
    </PageBody>
  )
}
