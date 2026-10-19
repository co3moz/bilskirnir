import * as React from "react"
import { RiAlertLine, RiCloseLine, RiComputerLine } from "@remixicon/react"
import { toast } from "sonner"

import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
  FieldLegend,
  FieldSet,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { closeDialog, openDialog } from "@/lib/dialogs"
import { rich, t } from "@/lib/i18n"
import {
  ALIAS_PATTERN,
  ENDPOINT_NAME_PATTERN,
  endpointUrl,
  grantDraft,
  grantRequests,
  newGrant,
  orderScopes,
  scopeDescription,
  scopeLabel,
  SCOPES,
  type GrantDraft,
} from "@/lib/mcp"
import { createEndpoint, updateEndpoint } from "@/lib/mcp-api"
import { useContainers, useMcpEndpoints, useMcpScopes, useMcpServers } from "@/lib/queries"
import type { ContainerItem, McpScope, McpScopes } from "@/lib/types"
import { useSubmit } from "./container-dialogs"
import type { DialogProps } from "./side-drawer"
import { Footer } from "./simple-dialogs"

/** One container of an endpoint: the name agents know it by, its scopes and its servers. */
function GrantFields({
  draft,
  onChange,
  onRemove,
  container,
  servers,
  scopes,
}: {
  draft: GrantDraft
  onChange: (draft: GrantDraft) => void
  onRemove: () => void
  /** The container, or nothing once it was deleted. */
  container?: ContainerItem
  /** The names of the container's desktop MCP servers. */
  servers: string[]
  scopes?: McpScopes
}) {
  const id = React.useId()
  const set = (patch: Partial<GrantDraft>) => onChange({ ...draft, ...patch })
  const name = container?.spec.name ?? draft.container_name
  // Every scope this release knows, and any a newer manager offers besides.
  const offered = orderScopes([...SCOPES, ...(scopes?.scopes.map((scope) => scope.id) ?? [])])
  const tools = (scope: McpScope) =>
    scopes?.scopes.find((entry) => entry.id === scope)?.tools.join(", ")
  // Servers chosen earlier stay listed after they were removed, so they can be taken out.
  const names = [...new Set([...servers, ...draft.servers])]
  const toggleScope = (scope: McpScope, on: boolean) =>
    set({
      scopes: on
        ? orderScopes([...draft.scopes, scope])
        : draft.scopes.filter((other) => other !== scope),
    })
  const toggleServer = (server: string, on: boolean) =>
    set({
      servers: on
        ? names.filter((other) => other === server || draft.servers.includes(other))
        : draft.servers.filter((other) => other !== server),
    })

  return (
    <div className="flex flex-col gap-4 rounded-2xl border p-4">
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <RiComputerLine className="size-4 shrink-0 text-muted-foreground" aria-hidden />
          <span className="text-sm font-medium break-all">{name}</span>
          {!container && (
            <Badge variant="outline" className="text-warning">
              {t("mcp.grant.deleted")}
            </Badge>
          )}
        </div>
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          aria-label={t("mcp.grant.removeOf", { name })}
          onClick={onRemove}
        >
          <RiCloseLine />
        </Button>
      </div>
      <Field>
        <FieldLabel htmlFor={`${id}-alias`}>{t("mcp.grant.alias")}</FieldLabel>
        <Input
          id={`${id}-alias`}
          required
          pattern={ALIAS_PATTERN}
          title={t("mcp.namePattern")}
          value={draft.alias}
          onChange={(event) => set({ alias: event.target.value })}
          className="font-mono"
        />
        <FieldDescription className="text-xs">{t("mcp.grant.aliasHint")}</FieldDescription>
      </Field>
      <FieldSet className="gap-3">
        <FieldLegend variant="label" className="mb-0">
          {t("mcp.grant.scopes")}
        </FieldLegend>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          {offered.map((scope) => (
            <Field key={scope} orientation="horizontal" className="items-start">
              <Checkbox
                id={`${id}-${scope}`}
                checked={draft.scopes.includes(scope)}
                onCheckedChange={(checked) => toggleScope(scope, checked === true)}
              />
              <FieldContent>
                <FieldLabel htmlFor={`${id}-${scope}`} title={tools(scope)}>
                  {scopeLabel(scope)}
                </FieldLabel>
                {scopeDescription(scope) && (
                  <FieldDescription className="text-xs">{scopeDescription(scope)}</FieldDescription>
                )}
              </FieldContent>
            </Field>
          ))}
        </div>
        {draft.scopes.includes("commands") && container?.spec.sudo && (
          <Alert>
            <RiAlertLine className="text-warning" />
            <AlertDescription>{t("mcp.grant.sudoWarning", { name })}</AlertDescription>
          </Alert>
        )}
        <FieldDescription className="text-xs">{t("mcp.grant.never")}</FieldDescription>
      </FieldSet>
      <FieldSet className="gap-3">
        <FieldLegend variant="label" className="mb-0" id={`${id}-servers`}>
          {t("mcp.grant.servers")}
        </FieldLegend>
        <RadioGroup
          aria-labelledby={`${id}-servers`}
          value={draft.all ? "all" : "chosen"}
          onValueChange={(value) => set({ all: value === "all" })}
        >
          <Label className="flex items-center gap-2 font-normal">
            <RadioGroupItem value="all" />
            {t("mcp.grant.allServers")}
          </Label>
          <Label className="flex items-center gap-2 font-normal">
            <RadioGroupItem value="chosen" />
            {t("mcp.grant.chosenServers")}
          </Label>
        </RadioGroup>
        {!draft.all &&
          (names.length ? (
            <div className="flex flex-col gap-2 pl-6">
              {names.map((server) => (
                <Label key={server} className="flex items-center gap-2 font-normal">
                  <Checkbox
                    checked={draft.servers.includes(server)}
                    onCheckedChange={(checked) => toggleServer(server, checked === true)}
                  />
                  <code className="font-mono text-xs">{server}</code>
                </Label>
              ))}
            </div>
          ) : (
            <FieldDescription className="pl-6 text-xs">{t("mcp.grant.noServers")}</FieldDescription>
          ))}
      </FieldSet>
    </div>
  )
}

/**
 * Creates an MCP endpoint, or changes which containers an existing one reaches and what it may do
 * there. Its name is its URL, so an existing endpoint keeps it.
 */
export function EndpointDialog({ name: existing, ...props }: DialogProps & { name?: string }) {
  const endpoints = useMcpEndpoints(false).data
  // The endpoint as the table showed it; its grants fill the form once.
  const [endpoint] = React.useState(() =>
    existing ? endpoints?.find((entry) => entry.name === existing) : undefined
  )
  const scopes = useMcpScopes().data
  const containers = useContainers().data ?? []
  const servers = useMcpServers().data ?? []
  const [name, setName] = React.useState(existing ?? "")
  const [grants, setGrants] = React.useState<GrantDraft[]>(
    () => endpoint?.grants.map(grantDraft) ?? []
  )
  const { busy, error, submit } = useSubmit()
  const id = React.useId()
  const available = containers.filter(
    (container) => !grants.some((grant) => grant.container_id === container.id)
  )
  const add = (containerId: string) => {
    const container = containers.find((entry) => entry.id === containerId)
    if (!container) return
    // Without the manager's defaults the new container starts with the least: viewing.
    const defaults = scopes?.default ?? ["view"]
    setGrants([
      ...grants,
      newGrant(
        { id: container.id, name: container.spec.name },
        defaults,
        grants.map((grant) => grant.alias)
      ),
    ])
  }
  const save = async () => {
    const body = grantRequests(grants)
    if (existing) {
      await updateEndpoint(existing, body)
      closeDialog()
      toast.success(t("mcp.endpoint.saved", { name: existing }))
      return
    }
    // The answer holds the token, which is shown once.
    openDialog({ kind: "mcp-token", token: await createEndpoint(name, body) })
  }

  if (existing && !endpoint)
    return (
      <Dialog {...props}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("mcp.endpoint.edit")}</DialogTitle>
            <DialogDescription>{t("mcp.endpoint.gone")}</DialogDescription>
          </DialogHeader>
        </DialogContent>
      </Dialog>
    )
  return (
    <Dialog {...props}>
      {/* The dialog stays inside the window: its body scrolls and the buttons stay in view. */}
      <DialogContent className="flex max-h-[calc(100dvh-2rem)] flex-col sm:max-w-2xl">
        <form className="flex min-h-0 flex-col gap-4" onSubmit={submit(save)}>
          <DialogHeader>
            <DialogTitle>
              {existing ? t("mcp.endpoint.editOf", { name: existing }) : t("mcp.endpoint.new")}
            </DialogTitle>
            <DialogDescription>{t("mcp.endpoint.formDescription")}</DialogDescription>
          </DialogHeader>
          <FieldGroup className="-mx-4 min-h-0 w-auto overflow-y-auto px-4 py-1">
            <Field>
              <FieldLabel htmlFor={`${id}-name`}>{t("table.name")}</FieldLabel>
              <Input
                id={`${id}-name`}
                required
                readOnly={!!existing}
                pattern={ENDPOINT_NAME_PATTERN}
                title={t("mcp.namePattern")}
                placeholder="unity-agent"
                value={name}
                onChange={(event) => setName(event.target.value)}
                className="font-mono"
                autoFocus={!existing}
              />
              <FieldDescription className="text-xs break-all">
                {rich(existing ? "mcp.endpoint.urlFixed" : "mcp.endpoint.urlHint", {
                  url: (
                    <code className="font-mono text-foreground">
                      {endpointUrl(location.origin, `/mcp/${name || "…"}`)}
                    </code>
                  ),
                })}
              </FieldDescription>
            </Field>
            <FieldSet className="gap-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <FieldLegend variant="label" className="mb-0">
                  {t("mcp.endpoint.containers")}
                </FieldLegend>
                <Select value="" onValueChange={add} disabled={!available.length}>
                  <SelectTrigger
                    size="sm"
                    className="w-full sm:w-64"
                    aria-label={t("mcp.grant.add")}
                  >
                    <SelectValue
                      placeholder={
                        available.length ? t("mcp.grant.addPlaceholder") : t("mcp.grant.allAdded")
                      }
                    />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {available.map((container) => (
                        <SelectItem key={container.id} value={container.id}>
                          {container.spec.name}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
              </div>
              {grants.length ? (
                grants.map((grant, index) => (
                  <GrantFields
                    key={grant.container_id}
                    draft={grant}
                    container={containers.find((entry) => entry.id === grant.container_id)}
                    servers={servers
                      .filter((server) => server.container_id === grant.container_id)
                      .map((server) => server.name)}
                    scopes={scopes}
                    onChange={(next) =>
                      setGrants(grants.map((other, at) => (at === index ? next : other)))
                    }
                    onRemove={() => setGrants(grants.filter((_, at) => at !== index))}
                  />
                ))
              ) : (
                <FieldDescription className="text-xs">{t("mcp.endpoint.addHint")}</FieldDescription>
              )}
            </FieldSet>
          </FieldGroup>
          {/* Outside the scrolling fields, so a long form cannot hide the outcome. */}
          <FieldError>{error}</FieldError>
          <Footer
            busy={busy}
            label={existing ? t("common.save") : t("common.create")}
            disabled={!grants.length}
          />
        </form>
      </DialogContent>
    </Dialog>
  )
}
