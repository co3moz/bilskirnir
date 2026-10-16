import * as React from "react"
import {
  columnVisibilityFeature,
  createColumnHelper,
  createPaginatedRowModel,
  createSortedRowModel,
  FlexRender,
  rowPaginationFeature,
  rowSelectionFeature,
  rowSortingFeature,
  tableFeatures,
  useTable,
  type Column,
  type ColumnDef,
  type ReactTable,
  type RowData,
  type SortingState,
} from "@tanstack/react-table"
import {
  RiArrowDownLine,
  RiArrowUpDownLine,
  RiArrowUpLine,
  RiArrowDownSLine,
  RiArrowLeftSLine,
  RiArrowRightSLine,
  RiArrowLeftDoubleLine,
  RiArrowRightDoubleLine,
  RiLayoutColumnLine,
} from "@remixicon/react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { TabsList, TabsTrigger } from "@/components/ui/tabs"
import { formatNumber, plural, t } from "@/lib/i18n"

// Declare the features the tables use; anything not registered is tree-shaken out of the bundle.
export const features = tableFeatures({
  columnVisibilityFeature,
  rowPaginationFeature,
  rowSelectionFeature,
  rowSortingFeature,
  paginatedRowModel: createPaginatedRowModel(),
  sortedRowModel: createSortedRowModel(),
})
export type Features = typeof features
export type DataTable<T extends RowData> = ReactTable<Features, T>
// Columns of different value types share one array; TanStack's own `columns` helper types them so.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type DataColumn<T extends RowData> = ColumnDef<Features, T, any>

export const columnHelper = <T extends RowData>() => createColumnHelper<Features, T>()

export function useDataTable<T extends RowData>({
  data,
  columns,
  getRowId,
  sorting = [],
  selectable = false,
  hidden = {},
}: {
  data: T[]
  columns: DataColumn<T>[]
  getRowId: (row: T) => string
  sorting?: SortingState
  selectable?: boolean
  hidden?: Record<string, boolean>
}) {
  return useTable({
    features,
    data,
    columns,
    getRowId,
    enableRowSelection: selectable,
    // Lists refresh every few seconds; a refresh must not send the reader back to page one.
    autoResetPageIndex: false,
    initialState: {
      sorting,
      pagination: { pageIndex: 0, pageSize: 10 },
      columnVisibility: Object.fromEntries(Object.keys(hidden).map((key) => [key, !hidden[key]])),
    },
  })
}

/** The row selection column from dashboard-01. */
export function selectColumn<T extends RowData>(): DataColumn<T> {
  return columnHelper<T>().display({
    id: "select",
    header: ({ table }) => (
      <div className="flex items-center justify-center">
        <Checkbox
          checked={
            table.getIsAllPageRowsSelected() ||
            (table.getIsSomePageRowsSelected() && "indeterminate")
          }
          onCheckedChange={(value) => table.toggleAllPageRowsSelected(!!value)}
          aria-label={t("table.selectAll")}
        />
      </div>
    ),
    cell: ({ row }) => (
      <div className="flex items-center justify-center">
        <Checkbox
          checked={row.getIsSelected()}
          onCheckedChange={(value) => row.toggleSelected(!!value)}
          aria-label={t("table.selectRow")}
        />
      </div>
    ),
    enableSorting: false,
    enableHiding: false,
  })
}

export function SortHeader<T extends RowData>({
  column,
  children,
  className,
}: {
  column: Column<Features, T>
  children: React.ReactNode
  className?: string
}) {
  const sorted = column.getIsSorted()
  return (
    <Button
      variant="ghost"
      size="sm"
      className={`-ml-2 h-7 data-[sorted=true]:text-foreground ${className ?? ""}`}
      data-sorted={!!sorted}
      onClick={column.getToggleSortingHandler()}
    >
      {children}
      {sorted === "asc" ? (
        <RiArrowUpLine />
      ) : sorted === "desc" ? (
        <RiArrowDownLine />
      ) : (
        <RiArrowUpDownLine className="opacity-40" />
      )}
    </Button>
  )
}

export function ColumnsMenu<T extends RowData>({
  table,
  labels,
}: {
  table: DataTable<T>
  labels: Record<string, string>
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" size="sm">
          <RiLayoutColumnLine data-icon="inline-start" />
          <span className="hidden lg:inline">{t("table.columns")}</span>
          <RiArrowDownSLine data-icon="inline-end" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-40">
        {table
          .getAllColumns()
          .filter((column) => column.getCanHide() && labels[column.id])
          .map((column) => (
            <DropdownMenuCheckboxItem
              key={column.id}
              checked={column.getIsVisible()}
              onCheckedChange={(value) => column.toggleVisibility(!!value)}
              onSelect={(event) => event.preventDefault()}
            >
              {labels[column.id]}
            </DropdownMenuCheckboxItem>
          ))}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

export function DataTableView<T extends RowData>({
  table,
  empty,
}: {
  table: DataTable<T>
  empty: React.ReactNode
}) {
  const rows = table.getRowModel().rows
  return (
    <div className="overflow-hidden rounded-lg border">
      <Table>
        <TableHeader className="sticky top-0 z-10 bg-muted">
          {table.getHeaderGroups().map((headerGroup) => (
            <TableRow key={headerGroup.id}>
              {headerGroup.headers.map((header) => (
                <TableHead key={header.id} colSpan={header.colSpan}>
                  {header.isPlaceholder ? null : <FlexRender header={header} />}
                </TableHead>
              ))}
            </TableRow>
          ))}
        </TableHeader>
        <TableBody className="**:data-[slot=table-cell]:first:w-8">
          {rows.length ? (
            rows.map((row) => (
              <TableRow key={row.id} data-state={row.getIsSelected() && "selected"}>
                {row.getVisibleCells().map((cell) => (
                  <TableCell key={cell.id}>
                    <FlexRender cell={cell} />
                  </TableCell>
                ))}
              </TableRow>
            ))
          ) : (
            <TableRow>
              <TableCell
                colSpan={table.getVisibleLeafColumns().length}
                className="h-24 text-center whitespace-normal text-muted-foreground"
              >
                {empty}
              </TableCell>
            </TableRow>
          )}
        </TableBody>
      </Table>
    </div>
  )
}

export function DataTablePagination<T extends RowData>({
  table,
  selectable = false,
}: {
  table: DataTable<T>
  selectable?: boolean
}) {
  const id = React.useId()
  const { pageIndex, pageSize } = table.state.pagination
  return (
    <div className="flex items-center justify-between px-4">
      <div className="hidden flex-1 text-sm text-muted-foreground lg:flex">
        {selectable
          ? t("table.selected", {
              selected: formatNumber(table.getSelectedRowModel().rows.length),
              total: formatNumber(table.getRowCount()),
            })
          : plural(table.getRowCount(), { one: "table.rows.one", other: "table.rows.other" })}
      </div>
      <div className="flex w-full items-center gap-8 lg:w-fit">
        <div className="hidden items-center gap-2 lg:flex">
          <Label htmlFor={`${id}-rows`} className="text-sm font-medium">
            {t("table.rowsPerPage")}
          </Label>
          <Select value={`${pageSize}`} onValueChange={(value) => table.setPageSize(Number(value))}>
            <SelectTrigger size="sm" className="w-20" id={`${id}-rows`}>
              <SelectValue placeholder={pageSize} />
            </SelectTrigger>
            <SelectContent side="top">
              <SelectGroup>
                {[10, 20, 30, 50, 100].map((size) => (
                  <SelectItem key={size} value={`${size}`}>
                    {size}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </div>
        <div className="flex w-fit items-center justify-center text-sm font-medium">
          {t("table.page", {
            page: formatNumber(Math.min(pageIndex + 1, Math.max(table.getPageCount(), 1))),
            pages: formatNumber(Math.max(table.getPageCount(), 1)),
          })}
        </div>
        <div className="ml-auto flex items-center gap-2 lg:ml-0">
          <Button
            variant="outline"
            className="hidden h-8 w-8 p-0 lg:flex"
            onClick={() => table.setPageIndex(0)}
            disabled={!table.getCanPreviousPage()}
          >
            <span className="sr-only">{t("table.first")}</span>
            <RiArrowLeftDoubleLine />
          </Button>
          <Button
            variant="outline"
            className="size-8"
            size="icon"
            onClick={() => table.previousPage()}
            disabled={!table.getCanPreviousPage()}
          >
            <span className="sr-only">{t("table.previous")}</span>
            <RiArrowLeftSLine />
          </Button>
          <Button
            variant="outline"
            className="size-8"
            size="icon"
            onClick={() => table.nextPage()}
            disabled={!table.getCanNextPage()}
          >
            <span className="sr-only">{t("table.next")}</span>
            <RiArrowRightSLine />
          </Button>
          <Button
            variant="outline"
            className="hidden size-8 lg:flex"
            size="icon"
            onClick={() => table.setPageIndex(table.getPageCount() - 1)}
            disabled={!table.getCanNextPage()}
          >
            <span className="sr-only">{t("table.last")}</span>
            <RiArrowRightDoubleLine />
          </Button>
        </div>
      </div>
    </div>
  )
}

/**
 * The view switch from dashboard-01: tabs on wide layouts, a select on narrow ones. Render it
 * inside a `Tabs` that holds the same value.
 */
export function ViewTabs({
  value,
  onChange,
  views,
}: {
  value: string
  onChange: (value: string) => void
  views: { value: string; label: string; count?: number }[]
}) {
  const id = React.useId()
  return (
    <>
      <Label htmlFor={id} className="sr-only">
        {t("table.view")}
      </Label>
      <Select value={value} onValueChange={onChange}>
        <SelectTrigger className="flex w-fit @4xl/main:hidden" size="sm" id={id}>
          <SelectValue placeholder={t("table.selectView")} />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            {views.map((view) => (
              <SelectItem key={view.value} value={view.value}>
                {view.label}
                {view.count ? ` (${view.count})` : ""}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
      <TabsList className="hidden **:data-[slot=badge]:size-5 **:data-[slot=badge]:rounded-full **:data-[slot=badge]:bg-muted-foreground/30 **:data-[slot=badge]:px-1 @4xl/main:flex">
        {views.map((view) => (
          <TabsTrigger key={view.value} value={view.value}>
            {view.label}
            {!!view.count && <Badge variant="secondary">{view.count}</Badge>}
          </TabsTrigger>
        ))}
      </TabsList>
    </>
  )
}
