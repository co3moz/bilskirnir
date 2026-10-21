import * as React from "react"
import {
  RiCheckboxCircleFill,
  RiCloseCircleLine,
  RiDownloadLine,
  RiErrorWarningLine,
} from "@remixicon/react"

import { CodeReader } from "@/components/code-editor"
import { CopyButton } from "@/components/copy-button"
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Progress as ProgressBar } from "@/components/ui/progress"
import { Spinner } from "@/components/ui/spinner"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { saveData } from "@/lib/actions"
import { formatDate, formatNumber, t } from "@/lib/i18n"
import {
  asJson,
  base64Bytes,
  mediaSource,
  requestDuration,
  resultFile,
  sameJson,
  shortenStrings,
  type ItemKind,
} from "@/lib/inspector"
import type { Progress } from "@/lib/mcp-client"
import { isPlainObject, prettyJson } from "@/lib/schema-form"
import { fileSize } from "@/lib/uploads"

/** A run of an item: going on, or how it ended. */
export type RunState =
  | { status: "running"; started: number; progress?: Progress; controller: AbortController }
  | {
      status: "done"
      ok: boolean
      /** The request's `result`, as the server sent it. */
      result?: unknown
      error?: string
      durationMs: number
      /** Milliseconds since the epoch. */
      at: number
      /** The URI a resource or a template was read at. */
      uri?: string
    }

type Block = Record<string, unknown>
const blocks = (value: unknown): Block[] =>
  Array.isArray(value) ? value.filter(isPlainObject) : []
const text = (value: unknown) => (typeof value === "string" ? value : "")

/** What a request is doing or how it ended, with the result laid out for its kind. */
export function ResultView({
  kind,
  itemKey,
  run,
}: {
  kind: ItemKind
  itemKey: string
  run: RunState
}) {
  if (run.status === "running") return <Running run={run} />
  const toolError =
    run.ok && kind === "tool" && isPlainObject(run.result) && run.result.isError === true
  return (
    <section className="flex flex-col gap-3" aria-live="polite">
      <div className="flex flex-wrap items-center gap-2">
        <h3 className="text-sm font-medium">{t("inspector.result.title")}</h3>
        {!run.ok ? (
          <Badge variant="destructive">
            <RiCloseCircleLine />
            {t("inspector.result.failed")}
          </Badge>
        ) : toolError ? (
          <Badge variant="outline" className="text-warning">
            <RiErrorWarningLine />
            {t("inspector.result.toolError")}
          </Badge>
        ) : (
          <Badge variant="outline" className="text-muted-foreground">
            <RiCheckboxCircleFill className="text-success" />
            {t("inspector.result.success")}
          </Badge>
        )}
        <span className="text-xs text-muted-foreground">
          {requestDuration(run.durationMs)} ·{" "}
          {formatDate(run.at / 1000, { hour: "numeric", minute: "2-digit", second: "2-digit" })}
        </span>
      </div>
      {run.uri && (
        <p className="text-xs text-muted-foreground">
          {t("inspector.result.uri")}{" "}
          <code className="font-mono break-all text-foreground">{run.uri}</code>
        </p>
      )}
      {!run.ok ? (
        <pre className="rounded-xl border border-destructive/40 bg-destructive/10 p-3 font-mono text-xs break-words whitespace-pre-wrap text-destructive">
          {run.error}
        </pre>
      ) : kind === "tool" ? (
        <ToolResult result={run.result} />
      ) : kind === "prompt" ? (
        <PromptResult result={run.result} />
      ) : (
        <ResourceResult result={run.result} />
      )}
      {run.ok && (
        <Accordion type="multiple" className="rounded-xl border px-3">
          <AccordionItem value="raw">
            <AccordionTrigger className="py-2 text-xs">
              {t("inspector.result.raw")}
            </AccordionTrigger>
            <AccordionContent className="flex flex-col gap-2 pt-0 pb-3">
              <p className="text-xs text-muted-foreground">{t("inspector.result.rawHint")}</p>
              <div className="flex gap-2">
                <CopyButton text={prettyJson(run.result)} label={t("inspector.result.copyRaw")} />
                <SaveButton
                  name={resultFile(kind, itemKey)}
                  data={prettyJson(run.result)}
                  type="application/json"
                  label={t("inspector.result.saveRaw")}
                />
              </div>
              <CodeReader
                language="json"
                value={prettyJson(shortenStrings(run.result))}
                label={t("inspector.result.raw")}
              />
            </AccordionContent>
          </AccordionItem>
        </Accordion>
      )}
    </section>
  )
}

/** Seconds since a run started, counted while it goes on. */
function Elapsed({ started }: { started: number }) {
  const [now, setNow] = React.useState(started)
  React.useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 100)
    return () => clearInterval(timer)
  }, [])
  return (
    <span className="font-mono text-xs text-muted-foreground tabular-nums">
      {t("inspector.result.seconds", {
        value: formatNumber(Math.max(0, now - started) / 1000, {
          minimumFractionDigits: 1,
          maximumFractionDigits: 1,
        }),
      })}
    </span>
  )
}

function Running({ run }: { run: Extract<RunState, { status: "running" }> }) {
  const progress = run.progress
  const percent =
    progress?.total && progress.total > 0
      ? Math.min(100, (progress.progress / progress.total) * 100)
      : undefined
  return (
    <section className="flex flex-col gap-2" aria-live="polite">
      <div className="flex items-center gap-2">
        <h3 className="text-sm font-medium">{t("inspector.result.title")}</h3>
        <Spinner />
        <span className="text-xs text-muted-foreground">{t("inspector.result.running")}</span>
        <Elapsed started={run.started} />
      </div>
      {progress && (
        <div className="flex flex-col gap-1">
          <ProgressBar
            value={percent ?? 100}
            className={percent === undefined ? "animate-pulse" : undefined}
          />
          <span className="text-xs text-muted-foreground">
            {[
              progress.total
                ? t("inspector.result.progressOf", {
                    progress: formatNumber(progress.progress),
                    total: formatNumber(progress.total),
                  })
                : formatNumber(progress.progress),
              progress.message,
            ]
              .filter(Boolean)
              .join(" · ")}
          </span>
        </div>
      )}
    </section>
  )
}

function SaveButton({
  name,
  data,
  type,
  label,
}: {
  name: string
  data: BlobPart
  type: string
  label: string
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="outline"
          size="icon-sm"
          aria-label={label}
          onClick={() => saveData(name, data, type)}
        >
          <RiDownloadLine />
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  )
}

/** One part of a result, with what it is and its actions above it. */
function Part({
  label,
  details,
  actions,
  children,
}: {
  label: React.ReactNode
  details?: React.ReactNode
  actions?: React.ReactNode
  children: React.ReactNode
}) {
  return (
    <div className="flex min-w-0 flex-col gap-2 rounded-xl border p-3">
      <div className="flex flex-wrap items-center gap-2">
        {label}
        {details && <span className="text-xs break-all text-muted-foreground">{details}</span>}
        {actions && <div className="ml-auto flex gap-2">{actions}</div>}
      </div>
      {children}
    </div>
  )
}

const Tag = ({ children }: { children: React.ReactNode }) => (
  <Badge variant="outline" className="font-mono text-muted-foreground">
    {children}
  </Badge>
)

/** Text, laid out as JSON when it is. */
function TextBody({ value }: { value: string }) {
  const json = asJson(value)
  if (json !== null)
    return <CodeReader language="json" value={json} label={t("inspector.result.title")} />
  return (
    <pre className="max-h-96 overflow-auto rounded-lg bg-muted/30 p-3 font-mono text-xs break-words whitespace-pre-wrap">
      {value}
    </pre>
  )
}

/** Base64 data: an image shown, anything else offered to save. */
function Binary({ mimeType, data, name }: { mimeType: unknown; data: string; name: string }) {
  const source = mediaSource(mimeType, data)
  if (source && String(mimeType).startsWith("image/"))
    return (
      <img
        src={source}
        alt={t("inspector.result.image")}
        className="max-h-96 w-fit max-w-full rounded-lg border object-contain"
      />
    )
  return (
    <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
      {t("inspector.result.binary", { size: fileSize(base64Bytes(data)) })}
      {source && (
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() =>
            saveData(
              name,
              Uint8Array.from(atob(data.replace(/\s/g, "")), (c) => c.charCodeAt(0)),
              String(mimeType)
            )
          }
        >
          <RiDownloadLine data-icon="inline-start" />
          {t("inspector.result.save")}
        </Button>
      )}
    </div>
  )
}

function ContentBlock({
  block,
  label,
  prefix,
}: {
  block: Block
  label?: string
  prefix?: React.ReactNode
}) {
  const type = text(block.type) || "unknown"
  const head = (
    <>
      {prefix}
      <Tag>{type}</Tag>
      {label && <span className="text-xs text-muted-foreground">{label}</span>}
    </>
  )
  switch (type) {
    case "text":
      return (
        <Part
          label={head}
          actions={<CopyButton text={text(block.text)} label={t("inspector.result.copyText")} />}
        >
          <TextBody value={text(block.text)} />
        </Part>
      )
    case "image":
    case "audio":
      return (
        <Part label={head} details={text(block.mimeType)}>
          <Binary mimeType={block.mimeType} data={text(block.data)} name={type} />
        </Part>
      )
    case "resource":
      return (
        <Part label={head}>
          <ResourceContents contents={isPlainObject(block.resource) ? block.resource : {}} />
        </Part>
      )
    case "resource_link":
      return (
        <Part label={head}>
          <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-3 gap-y-1 text-xs">
            <dt className="text-muted-foreground">{t("inspector.result.name")}</dt>
            <dd>{text(block.title) || text(block.name) || "-"}</dd>
            <dt className="text-muted-foreground">URI</dt>
            <dd>
              <code className="font-mono break-all">{text(block.uri)}</code>
            </dd>
            {text(block.mimeType) && (
              <>
                <dt className="text-muted-foreground">{t("inspector.result.mimeType")}</dt>
                <dd className="font-mono">{text(block.mimeType)}</dd>
              </>
            )}
            {text(block.description) && (
              <>
                <dt className="text-muted-foreground">{t("common.description")}</dt>
                <dd className="whitespace-pre-wrap">{text(block.description)}</dd>
              </>
            )}
          </dl>
        </Part>
      )
    default:
      return (
        <Part label={head}>
          <CodeReader
            language="json"
            value={prettyJson(shortenStrings(block))}
            label={t("inspector.result.title")}
          />
        </Part>
      )
  }
}

/** A text or blob resource, by its URI and type. */
function ResourceContents({ contents }: { contents: Block }) {
  const uri = text(contents.uri)
  const body = text(contents.text)
  const hasText = typeof contents.text === "string"
  return (
    <div className="flex min-w-0 flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <code className="font-mono text-xs break-all">{uri}</code>
        {text(contents.mimeType) && <Tag>{text(contents.mimeType)}</Tag>}
        {hasText && (
          <div className="ml-auto">
            <CopyButton text={body} label={t("inspector.result.copyText")} />
          </div>
        )}
      </div>
      {hasText ? (
        <TextBody value={body} />
      ) : typeof contents.blob === "string" ? (
        <Binary
          mimeType={contents.mimeType}
          data={contents.blob}
          name={uri.split("/").pop() || "resource"}
        />
      ) : (
        <CodeReader
          language="json"
          value={prettyJson(shortenStrings(contents))}
          label={t("inspector.result.title")}
        />
      )}
    </div>
  )
}

function ToolResult({ result }: { result: unknown }) {
  const value = isPlainObject(result) ? result : {}
  const content = blocks(value.content)
  const structured = value.structuredContent
  return (
    <div className="flex flex-col gap-2">
      {value.isError === true && (
        <p className="text-xs text-warning">{t("inspector.result.isError")}</p>
      )}
      {content.map((block, index) => {
        const label = content.length > 1 ? `#${index + 1}` : undefined
        // A tool with structuredContent usually repeats it as text for older clients.
        if (
          structured !== undefined &&
          block.type === "text" &&
          sameJson(text(block.text), structured)
        )
          return (
            <Accordion key={index} type="multiple" className="rounded-xl border px-3">
              <AccordionItem value="text">
                <AccordionTrigger className="py-2 text-xs">
                  <span className="flex flex-wrap items-center gap-2 font-normal">
                    <Tag>text</Tag>
                    {label}
                    <span className="text-muted-foreground">{t("inspector.result.sameJson")}</span>
                  </span>
                </AccordionTrigger>
                <AccordionContent className="pt-0 pb-3">
                  <TextBody value={text(block.text)} />
                </AccordionContent>
              </AccordionItem>
            </Accordion>
          )
        return <ContentBlock key={index} block={block} label={label} />
      })}
      {structured !== undefined && (
        <Part
          label={<Tag>structuredContent</Tag>}
          details={t("inspector.result.structured")}
          actions={
            <CopyButton
              text={prettyJson(structured)}
              label={t("inspector.result.copyStructured")}
            />
          }
        >
          <CodeReader language="json" value={prettyJson(structured)} label="structuredContent" />
        </Part>
      )}
      {!content.length && structured === undefined && (
        <p className="text-xs text-muted-foreground">{t("inspector.result.noContent")}</p>
      )}
    </div>
  )
}

function PromptResult({ result }: { result: unknown }) {
  const value = isPlainObject(result) ? result : {}
  const messages = blocks(value.messages)
  return (
    <div className="flex flex-col gap-2">
      {text(value.description) && (
        <p className="text-xs whitespace-pre-wrap text-muted-foreground">
          {text(value.description)}
        </p>
      )}
      {messages.map((message, index) => (
        <ContentBlock
          key={index}
          block={isPlainObject(message.content) ? message.content : {}}
          prefix={<Badge variant="secondary">{text(message.role) || "?"}</Badge>}
        />
      ))}
      {!messages.length && (
        <p className="text-xs text-muted-foreground">{t("inspector.result.noMessages")}</p>
      )}
    </div>
  )
}

function ResourceResult({ result }: { result: unknown }) {
  const contents = blocks(isPlainObject(result) ? result.contents : undefined)
  if (!contents.length)
    return <p className="text-xs text-muted-foreground">{t("inspector.result.noContents")}</p>
  return (
    <div className="flex flex-col gap-2">
      {contents.map((item, index) => (
        <div key={index} className="rounded-xl border p-3">
          <ResourceContents contents={item} />
        </div>
      ))}
    </div>
  )
}
