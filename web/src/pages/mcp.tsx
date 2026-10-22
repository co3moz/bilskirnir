import * as React from "react"
import {
  RiAddLine,
  RiComputerLine,
  RiDeleteBinLine,
  RiEditLine,
  RiForbidLine,
  RiInformationLine,
  RiKey2Line,
  RiMore2Line,
  RiPlayLine,
  RiPlugLine,
  RiRefreshLine,
  RiShareForwardLine,
} from "@remixicon/react"
import { cn } from "cn"
import { toast } from "sonner"

import { CopyButton } from "@/components/copy-button"
import { InspectorView } from "@/components/inspector/inspector-view"
import {
  columnHelper,
  DataTablePagination,
  DataTableView,
  SortHeader,
  useDataTable,
  ViewTabs,
  type DataColumn,
} from "@/components/data-table"
import { DockerAlert, PageBody } from "@/components/page"
import { StatusBadge } from "@/components/status-badge"
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty"
import { Spinner } from "@/components/ui/spinner"
import { Tabs, TabsContent } from "@/components/ui/tabs"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"
import { attempt } from "@/lib/actions"
import { wsUrl } from "@/lib/api"
import { openDialog } from "@/lib/dialogs"
import { containerStatus, shortDate } from "@/lib/format"
import { plural, rich, t } from "@/lib/i18n"
import { targetGroups, targetKey } from "@/lib/inspector"
import { INSPECT_PATH, InspectorClient, type InspectTarget } from "@/lib/mcp-client"
import {
  endpointUrl,
  scopeSummary,
  serverCommand,
  serverDetails,
  serversByContainer,
  serversSummary,
  serverTone,
} from "@/lib/mcp"
import { newToken, refreshServer, rereadServers } from "@/lib/mcp-api"
import {
  useContainers,
  useMcpEndpoints,
  useMcpScopes,
  useMcpServers,
  useSettings,
} from "@/lib/queries"
import type { ContainerItem, DesktopServer, McpEndpoint } from "@/lib/types"

type Row = McpEndpoint & {
  url: string
  /** The ids of the containers that still exist. */
  present: Set<string>
}
const column = columnHelper<Row>()

/** Opens a server in the inspector, from a row of the other views. */
const InspectContext = React.createContext<(target: InspectTarget) => void>(() => {})

function EndpointMenu({ endpoint }: { endpoint: McpEndpoint }) {
  const inspect = React.useContext(InspectContext)
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          className="flex size-8 text-muted-foreground data-[state=open]:bg-muted"
          size="icon"
        >
          <RiMore2Line />
          <span className="sr-only">{t("mcp.endpoint.actionsOf", { name: endpoint.name })}</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-52">
        <DropdownMenuItem
          tone="success"
          onSelect={() => inspect({ kind: "endpoint", name: endpoint.name })}
        >
          <RiPlayLine />
          {t("inspector.open")}
        </DropdownMenuItem>
        <DropdownMenuItem
          tone="info"
          onSelect={() => openDialog({ kind: "mcp-connect", name: endpoint.name })}
        >
          <RiShareForwardLine />
          {t("mcp.connect.action")}
        </DropdownMenuItem>
        <DropdownMenuItem
          tone="edit"
          onSelect={() => openDialog({ kind: "mcp-endpoint", name: endpoint.name })}
        >
          <RiEditLine />
          {t("mcp.endpoint.edit")}
        </DropdownMenuItem>
        <DropdownMenuItem tone="success" onSelect={() => attempt(() => newToken(endpoint))}>
          <RiKey2Line />
          {endpoint.has_token ? t("mcp.token.replace") : t("mcp.token.create")}
        </DropdownMenuItem>
        {endpoint.has_token && (
          <DropdownMenuItem
            tone="warning"
            onSelect={() =>
              openDialog({ kind: "mcp-endpoint-confirm", name: endpoint.name, action: "revoke" })
            }
          >
            <RiForbidLine />
            {t("mcp.token.revoke")}
          </DropdownMenuItem>
        )}
        <DropdownMenuSeparator />
        <DropdownMenuItem
          variant="destructive"
          onSelect={() =>
            openDialog({ kind: "mcp-endpoint-confirm", name: endpoint.name, action: "delete" })
          }
        >
          <RiDeleteBinLine />
          {t("mcp.endpoint.delete")}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

const columns: DataColumn<Row>[] = [
  column.accessor("name", {
    header: ({ column }) => <SortHeader column={column}>{t("table.name")}</SortHeader>,
    cell: ({ row }) => {
      const endpoint = row.original
      return (
        // Table cells do not wrap; the URL does here so the row's actions stay in view.
        <div className="flex min-w-56 flex-col gap-1 whitespace-normal">
          <div className="flex flex-wrap items-center gap-2">
            <RiPlugLine className="size-4 shrink-0 text-primary" aria-hidden />
            <span className="font-mono text-xs font-medium">{endpoint.name}</span>
            <Badge variant="outline" className="text-muted-foreground">
              {endpoint.automatic ? t("mcp.endpoint.automatic") : t("mcp.endpoint.custom")}
            </Badge>
          </div>
          <div className="flex items-center gap-1.5">
            <code className="font-mono text-[0.7rem] break-all text-muted-foreground">
              {endpoint.url}
            </code>
            <CopyButton
              size="icon-xs"
              text={endpoint.url}
              label={t("mcp.endpoint.copyUrlOf", { name: endpoint.name })}
            />
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="outline"
                  size="icon-xs"
                  className="shrink-0"
                  onClick={() => openDialog({ kind: "mcp-connect", name: endpoint.name })}
                  aria-label={t("mcp.connect.actionOf", { name: endpoint.name })}
                >
                  <RiShareForwardLine />
                </Button>
              </TooltipTrigger>
              <TooltipContent>{t("mcp.connect.action")}</TooltipContent>
            </Tooltip>
          </div>
        </div>
      )
    },
  }),
  column.display({
    id: "grants",
    header: () => t("mcp.endpoint.containers"),
    cell: ({ row }) => {
      const { grants, present } = row.original
      if (!grants.length)
        return <span className="text-xs text-muted-foreground">{t("mcp.endpoint.noGrants")}</span>
      return (
        <ul className="flex min-w-56 flex-col gap-1.5 text-xs whitespace-normal">
          {grants.map((grant) => (
            <li key={grant.container_id} className="flex flex-col gap-0.5">
              <span className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
                {present.has(grant.container_id) ? (
                  <button
                    type="button"
                    className="font-medium hover:underline"
                    onClick={() => openDialog({ kind: "details", id: grant.container_id })}
                  >
                    {grant.container_name}
                  </button>
                ) : (
                  <span className="font-medium">{grant.container_name}</span>
                )}
                <code className="font-mono text-muted-foreground">{grant.alias}</code>
                {!present.has(grant.container_id) && (
                  <Badge variant="outline" className="text-warning">
                    {t("mcp.grant.deleted")}
                  </Badge>
                )}
              </span>
              <span className="text-muted-foreground">{scopeSummary(grant.scopes)}</span>
              <span className="text-muted-foreground">
                {t("mcp.grant.serversLine", { servers: serversSummary(grant.servers) })}
              </span>
              {grant.excluded_tools.length > 0 && (
                <span className="text-muted-foreground" title={grant.excluded_tools.join(", ")}>
                  {plural(grant.excluded_tools.length, {
                    one: "mcp.grant.excluded.one",
                    other: "mcp.grant.excluded.other",
                  })}
                </span>
              )}
            </li>
          ))}
        </ul>
      )
    },
  }),
  column.display({
    id: "token",
    header: () => t("mcp.token.column"),
    cell: ({ row }) => {
      const endpoint = row.original
      return endpoint.has_token ? (
        <div className="flex flex-col gap-1">
          <StatusBadge state="token" tone="success" label={t("mcp.token.set")} />
          {endpoint.token_created_at !== null && (
            <span className="text-xs whitespace-nowrap text-muted-foreground">
              {shortDate(endpoint.token_created_at)}
            </span>
          )}
        </div>
      ) : (
        <span title={t("mcp.token.noneHint")}>
          <StatusBadge state="token" tone="muted" label={t("mcp.token.none")} />
        </span>
      )
    },
  }),
  column.accessor("tool_count", {
    header: () => t("mcp.endpoint.tools"),
    cell: ({ row }) => (
      <Button
        variant="link"
        className="h-auto p-0 text-xs whitespace-nowrap text-foreground"
        onClick={() => openDialog({ kind: "mcp-tools", name: row.original.name })}
      >
        {plural(row.original.tool_count, { one: "mcp.tools.one", other: "mcp.tools.other" })}
      </Button>
    ),
  }),
  column.display({
    id: "actions",
    cell: ({ row }) => {
      const endpoint = row.original
      return (
        <div className="flex items-center justify-end gap-2">
          {!endpoint.has_token && (
            <Button
              variant="outline"
              size="sm"
              onClick={() => attempt(() => newToken(endpoint))}
              aria-label={t("mcp.token.createOf", { name: endpoint.name })}
            >
              <RiKey2Line data-icon="inline-start" />
              {t("mcp.token.create")}
            </Button>
          )}
          <EndpointMenu endpoint={endpoint} />
        </div>
      )
    },
  }),
]

function EndpointsView({ containers }: { containers: ContainerItem[] }) {
  const endpoints = useMcpEndpoints()
  const data = React.useMemo<Row[]>(() => {
    const present = new Set(containers.map((container) => container.id))
    return (endpoints.data ?? []).map((endpoint) => ({
      ...endpoint,
      url: endpointUrl(location.origin, endpoint.path),
      present,
    }))
  }, [endpoints.data, containers])
  const table = useDataTable({
    data,
    columns,
    getRowId: (row) => row.name,
    sorting: [{ id: "name", desc: false }],
  })
  return (
    <>
      <DataTableView
        table={table}
        empty={
          endpoints.isPending
            ? t("mcp.endpoints.loading")
            : endpoints.error
              ? endpoints.error.message
              : t("mcp.endpoints.none")
        }
      />
      <DataTablePagination table={table} />
    </>
  )
}

/** Fetches a server's tools again, and says what came back. */
function RefreshServer({ server }: { server: DesktopServer }) {
  const [busy, setBusy] = React.useState(false)
  const refresh = async () => {
    setBusy(true)
    await attempt(async () => {
      const result = await refreshServer(server.container_id, server.name)
      if (result.state === "error")
        toast.error(t("mcp.server.failed", { name: result.name, error: result.error ?? "" }))
      else
        toast.success(
          plural(
            result.tools.length,
            { one: "mcp.server.read.one", other: "mcp.server.read.other" },
            { name: result.name }
          )
        )
    })
    setBusy(false)
  }
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="outline"
          size="icon-sm"
          disabled={busy || server.state === "stopped"}
          onClick={refresh}
          aria-label={t("mcp.server.refreshOf", { name: server.name })}
        >
          {busy ? <Spinner /> : <RiRefreshLine />}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{t("mcp.server.refresh")}</TooltipContent>
    </Tooltip>
  )
}

/** One desktop MCP server: where it comes from, how it starts, its state, tools and log. */
function ServerRow({ server }: { server: DesktopServer }) {
  const inspect = React.useContext(InspectContext)
  const details = serverDetails(server.env, server.headers, server.cwd)
  // Without a command or a URL its declaration could not be read, so nothing can start it.
  const startable = server.state !== "stopped" && !!(server.command || server.url)
  return (
    <li className="flex flex-col gap-2 px-4 py-3">
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-mono text-sm font-medium break-all">{server.name}</span>
            <Badge
              variant="outline"
              className={
                server.source === "image" ? "border-violet/40 text-violet" : "text-muted-foreground"
              }
              title={
                server.source === "image"
                  ? t("mcp.source.imageHint")
                  : t("mcp.source.containerHint")
              }
            >
              {server.source === "image" ? t("mcp.source.image") : t("mcp.source.container")}
            </Badge>
            <Badge variant="outline" className="text-muted-foreground">
              {server.transport === "stdio" ? "stdio" : "HTTP"}
            </Badge>
            <StatusBadge
              state={server.state}
              tone={serverTone(server.state)}
              label={t(`mcp.state.${server.state}`)}
            />
          </div>
          {server.description && (
            <p className="text-xs text-muted-foreground">{server.description}</p>
          )}
          <code className="font-mono text-[0.7rem] break-all text-muted-foreground">
            {serverCommand({
              command: server.command ?? undefined,
              args: server.args,
              url: server.url ?? undefined,
            })}
          </code>
          {details && <p className="text-xs break-words text-muted-foreground">{details}</p>}
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="outline"
                size="icon-sm"
                disabled={!startable}
                onClick={() =>
                  inspect({ kind: "server", container: server.container_id, name: server.name })
                }
                aria-label={t("inspector.openOf", { name: server.name })}
              >
                <RiPlayLine />
              </Button>
            </TooltipTrigger>
            <TooltipContent>{t("inspector.open")}</TooltipContent>
          </Tooltip>
          <RefreshServer server={server} />
          {server.source === "container" && (
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant="outline"
                  size="icon-sm"
                  className="text-destructive hover:text-destructive"
                  onClick={() =>
                    openDialog({
                      kind: "mcp-server-remove",
                      container: server.container_id,
                      name: server.name,
                    })
                  }
                  aria-label={t("mcp.server.removeOf", { name: server.name })}
                >
                  <RiDeleteBinLine />
                </Button>
              </TooltipTrigger>
              <TooltipContent>{t("mcp.server.remove")}</TooltipContent>
            </Tooltip>
          )}
        </div>
      </div>
      {server.error && (
        <p className="text-xs break-words whitespace-pre-wrap text-destructive">{server.error}</p>
      )}
      {server.tools.length || server.log ? (
        <Accordion type="multiple" className="rounded-xl border px-3">
          {server.tools.length > 0 && (
            <AccordionItem value="tools">
              <AccordionTrigger className="py-2 text-xs">
                <span className="flex flex-wrap items-center gap-x-2">
                  {plural(server.tools.length, { one: "mcp.tools.one", other: "mcp.tools.other" })}
                  {server.tools_at !== null && (
                    <span className="font-normal text-muted-foreground">
                      {t("mcp.server.readAt", { date: shortDate(server.tools_at) })}
                    </span>
                  )}
                </span>
              </AccordionTrigger>
              <AccordionContent className="pt-0 pb-2">
                <ul className="flex flex-col divide-y">
                  {server.tools.map((tool) => (
                    <li key={tool.name} className="flex flex-col gap-0.5 py-1.5">
                      <code className="font-mono text-xs break-all">{tool.name}</code>
                      {tool.description && (
                        <p className="line-clamp-3 text-xs text-muted-foreground">
                          {tool.description}
                        </p>
                      )}
                    </li>
                  ))}
                </ul>
              </AccordionContent>
            </AccordionItem>
          )}
          {server.log && (
            <AccordionItem value="log">
              <AccordionTrigger className="py-2 text-xs">{t("mcp.server.log")}</AccordionTrigger>
              <AccordionContent className="pt-0 pb-2">
                <pre className="max-h-64 overflow-auto rounded-md border bg-muted/40 p-3 font-mono text-xs leading-relaxed break-words whitespace-pre-wrap">
                  {server.log}
                </pre>
              </AccordionContent>
            </AccordionItem>
          )}
        </Accordion>
      ) : (
        <p className="text-xs text-muted-foreground">
          {server.state === "ready" ? t("mcp.server.noTools") : t("mcp.server.toolsUnread")}
        </p>
      )}
    </li>
  )
}

/** Reads a container's servers again from its desktop, such as ones added with bilskirnir-mcp. */
function RereadServers({ id, name }: { id: string; name: string }) {
  const [busy, setBusy] = React.useState(false)
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="outline"
          size="icon-sm"
          disabled={busy}
          onClick={async () => {
            setBusy(true)
            await attempt(() => rereadServers(id))
            setBusy(false)
          }}
          aria-label={t("mcp.servers.rereadOf", { name })}
        >
          {busy ? <Spinner /> : <RiRefreshLine />}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{t("mcp.servers.reread")}</TooltipContent>
    </Tooltip>
  )
}

function ServersView({ containers }: { containers: ContainerItem[] }) {
  const servers = useMcpServers()
  // Containers with servers come first; the others keep a short line each, with Add server.
  const groups = serversByContainer(servers.data ?? [], containers).sort(
    (a, b) => Number(!a.servers.length) - Number(!b.servers.length)
  )
  if (servers.isPending)
    return (
      <p className="flex items-center justify-center gap-2 py-12 text-sm text-muted-foreground">
        <Spinner />
        {t("mcp.servers.loading")}
      </p>
    )
  if (servers.error)
    return <p className="py-12 text-center text-sm text-destructive">{servers.error.message}</p>
  if (!groups.length)
    return (
      <Empty className="border">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <RiComputerLine />
          </EmptyMedia>
          <EmptyTitle>{t("containers.none")}</EmptyTitle>
          <EmptyDescription>{t("mcp.servers.noContainers")}</EmptyDescription>
        </EmptyHeader>
      </Empty>
    )
  return (
    <div className="flex flex-col gap-4">
      {groups.map((group) => {
        const status = group.container && containerStatus(group.container)
        const running = group.container?.state === "running"
        const name = group.container?.spec.name ?? group.name
        return (
          <section key={group.id} className="flex flex-col rounded-2xl border">
            <header
              className={cn(
                "flex flex-wrap items-center justify-between gap-2 px-4 py-3",
                group.servers.length > 0 && "border-b"
              )}
            >
              <div className="flex min-w-0 flex-wrap items-center gap-2">
                <RiComputerLine className="size-4 shrink-0 text-muted-foreground" aria-hidden />
                <h3 className="text-sm font-medium break-all">{name}</h3>
                {status ? (
                  <StatusBadge state={status.key} tone={status.tone} />
                ) : (
                  <Badge variant="outline" className="text-warning">
                    {t("mcp.grant.deleted")}
                  </Badge>
                )}
                {!group.servers.length && (
                  <span className="text-xs text-muted-foreground">{t("mcp.servers.noneHere")}</span>
                )}
              </div>
              {group.container && (
                <div className="flex items-center gap-2">
                  {running && <RereadServers id={group.id} name={name} />}
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={!running}
                    onClick={() => openDialog({ kind: "mcp-server", container: group.id })}
                    aria-label={t("mcp.server.addTo", { name })}
                  >
                    <RiAddLine data-icon="inline-start" />
                    {t("mcp.server.add")}
                  </Button>
                </div>
              )}
            </header>
            {group.servers.length > 0 && (
              <ul className="flex flex-col divide-y">
                {group.servers.map((server) => (
                  <ServerRow key={server.name} server={server} />
                ))}
              </ul>
            )}
          </section>
        )
      })}
    </div>
  )
}

type View = "endpoints" | "servers" | "inspector"

/** MCP endpoints, the desktop MCP servers whose tools they offer, and the inspector that runs them. */
export function McpPage() {
  const settings = useSettings().data
  const containers = useContainers().data
  const endpoints = useMcpEndpoints()
  const servers = useMcpServers()
  // Kept for the endpoint form, whose new containers start with the default scopes.
  useMcpScopes()
  const [view, setView] = React.useState<View>("endpoints")
  const list = React.useMemo(() => containers ?? [], [containers])
  const [client, setClient] = React.useState<InspectorClient | null>(null)
  const [picked, setPicked] = React.useState<string>()
  const groups = targetGroups(
    servers.data ?? [],
    endpoints.data ?? [],
    list,
    settings?.root_mcp_enabled !== false
  )
  // An inspector ends with the page, and the one before when another is opened.
  React.useEffect(() => () => client?.close(), [client])

  const connect = (target: InspectTarget) => {
    setClient(
      new InspectorClient(target, {
        socket: () => new WebSocket(wsUrl(INSPECT_PATH)),
        version: settings?.version ?? "",
      })
    )
    setPicked(targetKey(target))
  }
  const inspect = (target: InspectTarget) => {
    connect(target)
    setView("inspector")
  }
  const pickedTarget = groups
    .flatMap((group) => group.choices)
    .find((choice) => choice.key === picked)?.target

  return (
    <InspectContext.Provider value={inspect}>
      <PageBody>
        <DockerAlert />
        {settings?.root_mcp_enabled === false && (
          <div className="px-4 lg:px-6">
            <Alert>
              <RiInformationLine />
              <AlertTitle>{t("mcp.rootOff")}</AlertTitle>
              <AlertDescription>{t("mcp.rootOffHint")}</AlertDescription>
            </Alert>
          </div>
        )}
        <Tabs
          value={view}
          onValueChange={(value) => setView(value as View)}
          className="w-full flex-col justify-start gap-4"
        >
          <div className="flex flex-wrap items-center justify-between gap-2 px-4 lg:px-6">
            <ViewTabs
              value={view}
              onChange={(value) => setView(value as View)}
              views={[
                {
                  value: "endpoints",
                  label: t("mcp.view.endpoints"),
                  count: endpoints.data?.length,
                },
                { value: "servers", label: t("mcp.view.servers"), count: servers.data?.length },
                { value: "inspector", label: t("mcp.view.inspector") },
              ]}
            />
            {view === "endpoints" ? (
              <div className="flex flex-wrap items-center gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  disabled={settings?.root_mcp_enabled === false}
                  onClick={() => openDialog({ kind: "mcp-connect" })}
                >
                  <RiShareForwardLine data-icon="inline-start" />
                  {t("mcp.connect.admin")}
                </Button>
                <Button size="sm" onClick={() => openDialog({ kind: "mcp-endpoint" })}>
                  <RiAddLine data-icon="inline-start" />
                  {t("mcp.endpoint.new")}
                </Button>
              </div>
            ) : view === "servers" ? (
              <Button size="sm" onClick={() => openDialog({ kind: "mcp-server" })}>
                <RiAddLine data-icon="inline-start" />
                {t("mcp.server.add")}
              </Button>
            ) : null}
          </div>
          <TabsContent value="endpoints" className="flex flex-col gap-4 overflow-auto px-4 lg:px-6">
            <p className="text-sm text-muted-foreground">
              {rich("mcp.endpoints.description", {
                admin: <code className="font-mono text-foreground">/mcp</code>,
              })}
            </p>
            <EndpointsView containers={list} />
          </TabsContent>
          <TabsContent value="servers" className="flex flex-col gap-4 px-4 lg:px-6">
            <p className="text-sm text-muted-foreground">{t("mcp.servers.description")}</p>
            <ServersView containers={list} />
          </TabsContent>
          {/* Kept while another view shows, so the inspector's server and its results stay open. */}
          <TabsContent
            value="inspector"
            forceMount
            className="flex flex-col gap-4 px-4 data-[state=inactive]:hidden lg:px-6"
          >
            <p className="text-sm text-muted-foreground">{t("inspector.description")}</p>
            <InspectorView
              client={client}
              groups={groups}
              picked={picked}
              onPick={setPicked}
              onConnect={() => pickedTarget && connect(pickedTarget)}
              onDisconnect={() => client?.close()}
            />
          </TabsContent>
        </Tabs>
      </PageBody>
    </InspectContext.Provider>
  )
}
