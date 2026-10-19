import * as React from "react"
import { toast } from "sonner"

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
import { Textarea } from "@/components/ui/textarea"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { closeDialog } from "@/lib/dialogs"
import { plural, rich, t } from "@/lib/i18n"
import { SERVER_NAME_PATTERN, serverDraft, serverRequest, type ServerDraft } from "@/lib/mcp"
import { addServer } from "@/lib/mcp-api"
import { useContainers } from "@/lib/queries"
import { useSubmit } from "./container-dialogs"
import type { DialogProps } from "./side-drawer"
import { Footer } from "./simple-dialogs"

const code = (text: string) => <code className="font-mono text-foreground">{text}</code>

/**
 * Registers an MCP server in a running desktop: a command the manager starts there, or the URL
 * of a server that already listens inside it. Its tools are read before the dialog closes.
 */
export function ServerDialog({ container, ...props }: DialogProps & { container?: string }) {
  const containers = useContainers().data ?? []
  const [chosen, setChosen] = React.useState<string | undefined>(container)
  const target =
    chosen ?? containers.find((entry) => entry.state === "running")?.id ?? containers[0]?.id
  const item = containers.find((entry) => entry.id === target)
  const running = item?.state === "running"
  const [draft, setDraft] = React.useState<ServerDraft>(serverDraft)
  const set = (patch: Partial<ServerDraft>) => setDraft({ ...draft, ...patch })
  const { busy, error, submit } = useSubmit()
  const id = React.useId()

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
                <Select value={target ?? ""} onValueChange={setChosen}>
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
          <FieldError>{error}</FieldError>
          <Footer busy={busy} label={t("mcp.server.add")} disabled={!target || !running} />
        </form>
      </DialogContent>
    </Dialog>
  )
}
