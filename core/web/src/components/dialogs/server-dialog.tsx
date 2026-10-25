import * as React from "react"
import { RiCheckboxCircleFill, RiErrorWarningLine, RiPlayLine } from "@remixicon/react"
import { toast } from "sonner"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Spinner } from "@/components/ui/spinner"
import { Textarea } from "@/components/ui/textarea"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { wsUrl } from "@/lib/api"
import { closeDialog } from "@/lib/dialogs"
import { plural, rich, t } from "@/lib/i18n"
import { SERVER_NAME_PATTERN, serverDraft, serverRequest, type ServerDraft } from "@/lib/mcp"
import { addServer } from "@/lib/mcp-api"
import { INSPECT_PATH } from "@/lib/mcp-client"
import { useContainers, useSettings } from "@/lib/queries"
import { testServer, type TestOutcome } from "@/lib/server-test"
import { useSubmit } from "./container-dialogs"
import type { DialogProps } from "./side-drawer"
import { Footer } from "./simple-dialogs"

const code = (text: string) => <code className="font-mono text-foreground">{text}</code>

/** A try of the form's server: going on, with how to stop it, or how it went. */
type Test = { state: "running"; cancel(): void } | { state: "done"; outcome: TestOutcome }

/** What a try of the server found: its name and tools, or why it did not answer. */
function TestResult({ test, onCancel }: { test: Test; onCancel(): void }) {
  if (test.state === "running")
    return (
      <div aria-live="polite" className="flex items-center gap-2 text-xs text-muted-foreground">
        <Spinner />
        {t("mcp.server.testing")}
        <Button type="button" variant="ghost" size="sm" className="ml-auto" onClick={onCancel}>
          {t("common.cancel")}
        </Button>
      </div>
    )
  const { outcome } = test
  const output = outcome.output.length > 0 && (
    <div className="flex min-w-0 flex-col gap-1">
      <span className="text-xs text-muted-foreground">{t("mcp.server.testOutput")}</span>
      <pre className="max-h-32 overflow-auto rounded-lg border bg-muted/30 p-2 font-mono text-xs break-all whitespace-pre-wrap">
        {outcome.output.join("\n")}
      </pre>
    </div>
  )
  if (!outcome.ok)
    return (
      <Alert variant="destructive" aria-live="polite">
        <RiErrorWarningLine />
        <AlertTitle>{t("mcp.server.testFailed")}</AlertTitle>
        <AlertDescription className="flex min-w-0 flex-col gap-2">
          <span className="break-words whitespace-pre-wrap">{outcome.error}</span>
          {output}
        </AlertDescription>
      </Alert>
    )
  return (
    <div aria-live="polite" className="flex flex-col gap-2 rounded-xl border p-3 text-xs">
      <div className="flex flex-wrap items-center gap-2">
        <RiCheckboxCircleFill className="size-4 text-success" aria-hidden />
        <span className="font-medium">
          {t("mcp.server.testOk", {
            name: outcome.info?.title || outcome.info?.name || "?",
            version: outcome.info?.version ?? "",
          })}
        </span>
        <span className="text-muted-foreground">
          {plural(outcome.tools.length, { one: "mcp.tools.one", other: "mcp.tools.other" })}
        </span>
      </div>
      {outcome.tools.length > 0 && (
        <ul className="flex max-h-24 flex-wrap gap-1 overflow-auto">
          {outcome.tools.map((tool) => (
            <li key={tool.name}>
              <Badge
                variant="outline"
                className="font-mono text-muted-foreground"
                title={tool.description}
              >
                {tool.name}
              </Badge>
            </li>
          ))}
        </ul>
      )}
      <span className="text-muted-foreground">{t("mcp.server.testSaved")}</span>
      {output}
    </div>
  )
}

/**
 * Registers an MCP server in a running desktop: a command the manager starts there, or the URL
 * of a server that already listens inside it. Its tools are read before the dialog closes, and
 * **Test** tries the form as it is through the inspector's trial target, saving nothing.
 */
export function ServerDialog({ container, ...props }: DialogProps & { container?: string }) {
  const containers = useContainers().data ?? []
  const version = useSettings().data?.version ?? ""
  const [chosen, setChosen] = React.useState<string | undefined>(container)
  const target =
    chosen ?? containers.find((entry) => entry.state === "running")?.id ?? containers[0]?.id
  const item = containers.find((entry) => entry.id === target)
  const running = item?.state === "running"
  const [draft, setDraft] = React.useState<ServerDraft>(serverDraft)
  const [test, setTest] = React.useState<Test>()
  // A finished test speaks for the form as it was; a change makes it stale.
  const set = (patch: Partial<ServerDraft>) => {
    setDraft({ ...draft, ...patch })
    if (test?.state === "done") setTest(undefined)
  }
  const { busy, error, setError, submit } = useSubmit()
  const id = React.useId()
  // Closing the dialog stops a test still going on.
  React.useEffect(() => {
    if (test?.state === "running") return test.cancel
  }, [test])

  const startTest = () => {
    let request
    try {
      request = serverRequest(draft)
    } catch (failure) {
      setError((failure as Error).message)
      return
    }
    setError("")
    const run = testServer(target!, request, {
      socket: () => new WebSocket(wsUrl(INSPECT_PATH)),
      version,
    })
    setTest({ state: "running", cancel: run.cancel })
    void run.outcome.then((outcome) =>
      setTest((current) =>
        current?.state === "running" && current.cancel === run.cancel
          ? { state: "done", outcome }
          : current
      )
    )
  }

  const save = async () => {
    const request = serverRequest(draft)
    const server = await addServer(target!, request)
    closeDialog()
    if (server.state === "error")
      toast.error(t("mcp.server.failed", { name: server.name, error: server.error ?? "" }))
    else
      toast.success(
        plural(
          server.tools.length,
          { one: "mcp.server.added.one", other: "mcp.server.added.other" },
          { name: server.name }
        )
      )
  }

  return (
    <Dialog {...props}>
      <DialogContent className="flex max-h-[calc(100dvh-2rem)] flex-col sm:max-w-xl">
        <form className="flex min-h-0 flex-col gap-4" onSubmit={submit(save)}>
          <DialogHeader>
            <DialogTitle className="break-all">
              {container && item
                ? t("mcp.server.addTo", { name: item.spec.name })
                : t("mcp.server.add")}
            </DialogTitle>
            <DialogDescription>{t("mcp.server.formDescription")}</DialogDescription>
          </DialogHeader>
          <FieldGroup className="-mx-4 min-h-0 w-auto gap-5 overflow-y-auto px-4 py-1">
            {!container && (
              <Field>
                <FieldLabel htmlFor={`${id}-container`}>{t("containers.container")}</FieldLabel>
                <Select
                  value={target ?? ""}
                  onValueChange={(value) => {
                    setChosen(value)
                    if (test?.state === "done") setTest(undefined)
                  }}
                >
                  <SelectTrigger id={`${id}-container`} className="w-full">
                    <SelectValue placeholder={t("containers.none")} />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {containers.map((entry) => (
                        <SelectItem
                          key={entry.id}
                          value={entry.id}
                          disabled={entry.state !== "running"}
                        >
                          {entry.spec.name}
                          {entry.state === "running" ? "" : ` · ${t("usage.stopped")}`}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </Field>
            )}
            {item && !running && (
              <FieldDescription className="text-xs text-warning">
                {t("mcp.server.startFirst")}
              </FieldDescription>
            )}
            <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <Field>
                <FieldLabel htmlFor={`${id}-name`}>{t("table.name")}</FieldLabel>
                <Input
                  id={`${id}-name`}
                  required
                  pattern={SERVER_NAME_PATTERN}
                  title={t("mcp.namePattern")}
                  placeholder="playwright"
                  value={draft.name}
                  onChange={(event) => set({ name: event.target.value })}
                  className="font-mono"
                  autoFocus
                />
              </Field>
              <Field>
                <FieldLabel id={`${id}-transport`}>{t("mcp.server.transport")}</FieldLabel>
                <ToggleGroup
                  type="single"
                  variant="outline"
                  spacing={0}
                  value={draft.transport}
                  onValueChange={(value) =>
                    value && set({ transport: value as ServerDraft["transport"] })
                  }
                  aria-labelledby={`${id}-transport`}
                  className="w-full *:flex-1"
                >
                  <ToggleGroupItem value="stdio">{t("mcp.server.stdio")}</ToggleGroupItem>
                  <ToggleGroupItem value="http">{t("mcp.server.http")}</ToggleGroupItem>
                </ToggleGroup>
              </Field>
            </div>
            <Field>
              <FieldLabel htmlFor={`${id}-description`}>{t("common.description")}</FieldLabel>
              <Input
                id={`${id}-description`}
                maxLength={500}
                placeholder={t("common.optional")}
                value={draft.description}
                onChange={(event) => set({ description: event.target.value })}
              />
            </Field>
            {draft.transport === "stdio" ? (
              <>
                <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                  <Field>
                    <FieldLabel htmlFor={`${id}-command`}>{t("mcp.server.command")}</FieldLabel>
                    <Input
                      id={`${id}-command`}
                      required
                      spellCheck={false}
                      placeholder="npx"
                      value={draft.command}
                      onChange={(event) => set({ command: event.target.value })}
                      className="font-mono"
                    />
                  </Field>
                  <Field>
                    <FieldLabel htmlFor={`${id}-cwd`}>{t("mcp.server.cwd")}</FieldLabel>
                    <Input
                      id={`${id}-cwd`}
                      spellCheck={false}
                      placeholder={t("common.optional")}
                      value={draft.cwd}
                      onChange={(event) => set({ cwd: event.target.value })}
                      className="font-mono"
                    />
                  </Field>
                </div>
                <Field>
                  <FieldLabel htmlFor={`${id}-args`}>{t("mcp.server.args")}</FieldLabel>
                  <Textarea
                    id={`${id}-args`}
                    rows={3}
                    spellCheck={false}
                    placeholder={"-y\n@playwright/mcp@latest"}
                    value={draft.args}
                    onChange={(event) => set({ args: event.target.value })}
                    className="max-h-[500px] overflow-y-auto font-mono text-xs"
                  />
                  <FieldDescription className="text-xs">
                    {t("mcp.server.argsHint")}
                  </FieldDescription>
                </Field>
                <Field>
                  <FieldLabel htmlFor={`${id}-env`}>{t("mcp.server.env")}</FieldLabel>
                  <Textarea
                    id={`${id}-env`}
                    rows={3}
                    spellCheck={false}
                    placeholder={"API_KEY=${API_KEY}"}
                    value={draft.env}
                    onChange={(event) => set({ env: event.target.value })}
                    className="max-h-[500px] overflow-y-auto font-mono text-xs"
                  />
                  <FieldDescription className="text-xs">{t("mcp.server.envHint")}</FieldDescription>
                </Field>
              </>
            ) : (
              <>
                <Field>
                  <FieldLabel htmlFor={`${id}-url`}>{t("mcp.server.url")}</FieldLabel>
                  <Input
                    id={`${id}-url`}
                    required
                    spellCheck={false}
                    placeholder="http://127.0.0.1:8931/mcp"
                    value={draft.url}
                    onChange={(event) => set({ url: event.target.value })}
                    className="font-mono"
                  />
                  <FieldDescription className="text-xs">{t("mcp.server.urlHint")}</FieldDescription>
                </Field>
                <Field>
                  <FieldLabel htmlFor={`${id}-headers`}>{t("mcp.server.headers")}</FieldLabel>
                  <Textarea
                    id={`${id}-headers`}
                    rows={3}
                    spellCheck={false}
                    placeholder={"Authorization: Bearer ${TOKEN}"}
                    value={draft.headers}
                    onChange={(event) => set({ headers: event.target.value })}
                    className="max-h-[500px] overflow-y-auto font-mono text-xs"
                  />
                  <FieldDescription className="text-xs">
                    {t("mcp.server.headersHint")}
                  </FieldDescription>
                </Field>
              </>
            )}
            <FieldDescription className="text-xs">
              {rich("mcp.server.valuesHint", {
                value: code("${NAME}"),
                stepValue: code("${step.NAME}"),
              })}
            </FieldDescription>
            <FieldDescription className="text-xs">
              {rich("mcp.server.insideHint", {
                command: code("bilskirnir-mcp add NAME -- COMMAND ARGS"),
              })}
            </FieldDescription>
          </FieldGroup>
          {/* Dropping a running test stops it, through the effect above. */}
          {test && <TestResult test={test} onCancel={() => setTest(undefined)} />}
          <FieldError>{error}</FieldError>
          <Footer busy={busy} label={t("mcp.server.add")} disabled={!target || !running}>
            <Button
              type="button"
              variant="outline"
              disabled={!target || !running || test?.state === "running"}
              onClick={startTest}
            >
              <RiPlayLine data-icon="inline-start" />
              {t("mcp.server.test")}
            </Button>
          </Footer>
        </form>
      </DialogContent>
    </Dialog>
  )
}
