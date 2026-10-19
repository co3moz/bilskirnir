// What the MCP page changes through the manager's API. Each change refreshes the lists it affects:
// a desktop MCP server's tools count towards the endpoints that include it.
import { api } from "./api"
import { openDialog } from "./dialogs"
import { endpointApi, serverApi } from "./mcp"
import { queryClient } from "./queries"
import type {
  DesktopServer,
  EndpointToken,
  GrantRequest,
  McpEndpoint,
  ServerRequest,
} from "./types"

function refresh() {
  void queryClient.invalidateQueries({ queryKey: ["mcp", "servers"] })
  void queryClient.invalidateQueries({ queryKey: ["mcp", "endpoints"] })
}

async function changed<T>(request: Promise<T>) {
  const result = await request
  refresh()
  return result
}

/** Creates an endpoint; its token is in the answer, and only there. */
export const createEndpoint = (name: string, grants: GrantRequest[]) =>
  changed(api<EndpointToken>(endpointApi(), { name, grants }))

/** Replaces an endpoint's grants; its name, and so its URL, stays. */
export const updateEndpoint = (name: string, grants: GrantRequest[]) =>
  changed(api<McpEndpoint>(endpointApi(name), { grants }, "PUT"))

export const deleteEndpoint = (name: string) =>
  changed(api<{ deleted: boolean }>(endpointApi(name), undefined, "DELETE"))

/** Creates the endpoint's token, or replaces it: the old one stops working at once. */
export const createToken = (name: string) =>
  changed(api<EndpointToken>(`${endpointApi(name)}/token`, {}))

/** Revokes the endpoint's token; only the administrator token works there afterwards. */
export const revokeToken = (name: string) =>
  changed(api<McpEndpoint>(`${endpointApi(name)}/token`, undefined, "DELETE"))

/** Registers a desktop MCP server in a running container and reads its tools. */
export const addServer = (container: string, request: ServerRequest) =>
  changed(api<DesktopServer>(serverApi(container), request))

/** Removes a server registered in the container; one its image declares stays. */
export const removeServer = (container: string, name: string) =>
  changed(api<{ removed: boolean }>(serverApi(container, name), undefined, "DELETE"))

/** Reads a server's tools again. */
export const refreshServer = (container: string, name: string) =>
  changed(api<DesktopServer>(`${serverApi(container, name)}/refresh`, {}))

/** Reads a container's servers again from its running desktop, such as ones added inside it. */
export async function rereadServers(container: string) {
  const { items } = await api<{ items: DesktopServer[] }>(serverApi(container))
  queryClient.setQueryData<DesktopServer[]>(["mcp", "servers"], (current) => [
    ...(current ?? []).filter((server) => server.container_id !== container),
    ...items,
  ])
  void queryClient.invalidateQueries({ queryKey: ["mcp", "endpoints"] })
  return items
}

/** Creates an endpoint's first token and shows it; a token that exists is replaced after asking. */
export async function newToken(endpoint: Pick<McpEndpoint, "name" | "has_token">) {
  if (endpoint.has_token) {
    openDialog({ kind: "mcp-endpoint-confirm", name: endpoint.name, action: "replace" })
    return
  }
  openDialog({ kind: "mcp-token", token: await createToken(endpoint.name) })
}
