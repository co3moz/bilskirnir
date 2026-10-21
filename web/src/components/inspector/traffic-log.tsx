import * as React from "react"
import {
  RiArrowLeftDownLine,
  RiArrowRightUpLine,
  RiDeleteBinLine,
  RiInformationLine,
} from "@remixicon/react"
import { cn } from "cn"

import { CodeView } from "@/components/code-textarea"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { attempt } from "@/lib/actions"
import { formatDate, formatNumber, plural, t } from "@/lib/i18n"
import { asJson } from "@/lib/inspector"
import type { InspectorClient, LogEntry } from "@/lib/mcp-client"

type Filter = "all" | "messages" | "output" | "logs"
const FILTERS: Filter[] = ["all", "messages", "output", "logs"]
/** The levels of `logging/setLevel`, from the most to the least said. */
const LEVELS = ["debug", "info", "notice", "warning", "error", "critical", "alert", "emergency"]

const keeps = (filter: Filter, entry: LogEntry) =>
  filter === "all" ||
  (filter === "messages" && entry.kind === "message") ||
  (filter === "output" && (entry.kind === "stdout" || entry.kind === "stderr")) ||
  (filter === "logs" && entry.kind === "log")

/** Every message to and from the server, its output and its log, newest first. */
export function TrafficLog({
  client,
  log,
  logging,
}: {
  client: InspectorClient
  log: LogEntry[]
  /** Whether the server takes `logging/setLevel`. */
  logging: boolean
}) {
  const [filter, setFilter] = React.useState<Filter>("all")
  const shown = log.filter((entry) => keeps(filter, entry)).reverse()
  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <ToggleGroup
          type="single"
          variant="outline"
          size="sm"
          spacing={0}
          value={filter}
          onValueChange={(value) => value && setFilter(value as Filter)}
          aria-label={t("inspector.log.filter")}
        >
          {FILTERS.map((value) => (
            <ToggleGroupItem key={value} value={value}>
              {t(`inspector.log.${value}`)}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        {logging && (
          <Select onValueChange={(level) => attempt(() => client.setLogLevel(level))}>
            <SelectTrigger size="sm" className="w-44" aria-label={t("inspector.log.level")}>
              <SelectValue placeholder={t("inspector.log.level")} />
            </SelectTrigger>
            <SelectContent>
              <SelectGroup>
                {LEVELS.map((level) => (
                  <SelectItem key={level} value={level} className="font-mono text-xs">
                    {level}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        )}
        <span className="text-xs text-muted-foreground">
          {plural(shown.length, {
            one: "inspector.log.count.one",
            other: "inspector.log.count.other",
          })}
        </span>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="ml-auto"
          disabled={!log.length}
          onClick={() => client.clearLog()}
        >
          <RiDeleteBinLine data-icon="inline-start" />
          {t("inspector.log.clear")}
        </Button>
      </div>
      {shown.length ? (
        <ol className="flex max-h-[28rem] flex-col divide-y overflow-auto rounded-xl border">
          {shown.map((entry) => (
            <Row key={entry.id} entry={entry} />
          ))}
        </ol>
      ) : (
        <p className="text-xs text-muted-foreground">{t("inspector.log.empty")}</p>
      )}
    </div>
  )
}

const time = (at: number) =>
  `${formatDate(at / 1000, { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false })}.${String(at % 1000).padStart(3, "0")}`

function Direction({ entry }: { entry: LogEntry }) {
  const [Icon, label, tone] =
    entry.direction === "out"
      ? [RiArrowRightUpLine, t("inspector.log.sent"), "text-muted-foreground"]
      : entry.direction === "in"
        ? [RiArrowLeftDownLine, t("inspector.log.received"), "text-success"]
        : [RiInformationLine, t("inspector.log.note"), "text-muted-foreground"]
  return (
    <>
      <Icon aria-hidden className={cn("size-3.5 shrink-0", tone)} />
      <span className="sr-only">{label}</span>
    </>
  )
}

function Row({ entry }: { entry: LogEntry }) {
  const [open, setOpen] = React.useState(false)
  const json = open ? asJson(entry.text) : null
  const tone =
    entry.kind === "stderr" || entry.kind === "error" || entry.level === "error"
      ? "text-destructive"
      : entry.kind === "closed"
        ? "text-warning"
        : undefined
  return (
    <li className="min-w-0">
      <button
        type="button"
        className="flex w-full min-w-0 items-center gap-2 px-3 py-1.5 text-left text-xs hover:bg-muted/50 focus-visible:bg-muted/50 focus-visible:outline-none"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        <span className="shrink-0 font-mono text-muted-foreground tabular-nums">
          {time(entry.at)}
        </span>
        <Direction entry={entry} />
        <span className={cn("shrink-0 font-mono font-medium", tone)}>{entry.label}</span>
        {entry.level && (
          <Badge
            variant="outline"
            className="h-4 px-1.5 font-mono text-[0.65rem] text-muted-foreground"
          >
            {entry.level}
          </Badge>
        )}
        <span className="min-w-0 truncate font-mono text-muted-foreground">{entry.text}</span>
      </button>
      {open && (
        <div className="flex flex-col gap-1 px-3 pb-3">
          {json !== null ? (
            <CodeView language="json" value={json} className="max-h-72" />
          ) : (
            <pre className="max-h-72 overflow-auto rounded-lg bg-muted/30 p-2 font-mono text-xs break-words whitespace-pre-wrap">
              {entry.text}
            </pre>
          )}
          {entry.cut && (
            <p className="text-xs text-muted-foreground">
              {t("inspector.log.cut", { count: formatNumber(entry.text.length) })}
            </p>
          )}
        </div>
      )}
    </li>
  )
}
