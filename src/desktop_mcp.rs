//! MCP servers running inside desktops, and the scoped MCP endpoints that publish them.
//!
//! A desktop declares its MCP servers in JSON files, one per server: the image in
//! `/etc/bilskirnir/mcp.d`, which a build writes from a template's declarations, and the desktop
//! user in `~/.config/bilskirnir/mcp`, at any time. The manager watches both folders in every
//! running desktop and keeps the tool list it last read from each server, so the list is known
//! while the container is stopped too. A server is started only through `docker exec` as the
//! desktop user: a program speaks MCP on its stdio, and a Streamable HTTP server on the desktop's
//! loopback is reached through an adapter run the same way. No port is published and the
//! desktop's network is not involved.
//!
//! `/mcp` stays the administrator's channel. An endpoint at `/mcp/<name>` gives an agent the
//! container tools of the scopes it was granted, bound to its containers, and the tools of their
//! desktop servers, with a bearer token of its own that opens nothing else.
//!
//! The MCP page's inspector reaches any of these servers from the dashboard over a WebSocket.
//! Each running desktop is told the state of its servers and the endpoints that reach it, for
//! `bilskirnir-mcp` and the tray inside it.
mod client;
mod endpoints;
mod gateway;
mod inspect;
mod registry;
mod report;
mod scopes;
mod servers;
mod watch;

pub(crate) use endpoints::{
    ENDPOINTS, Endpoint, automatic_endpoint, create_endpoint, delete_endpoint, endpoint_name,
    endpoint_tools, endpoints, revoke_token, scopes_list, token, update_endpoint,
};
pub(crate) use gateway::endpoint;
pub(crate) use inspect::inspect;
#[cfg(test)]
pub(crate) use registry::SERVERS;
pub(crate) use registry::{Declaration, check_declarations, slug, write_declarations};
pub(crate) use servers::{
    all_servers, container_servers, refresh_server, register_server, unregister_server,
};
pub(crate) use watch::spawn;

use crate::App;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

/// What the manager holds in memory about desktop servers and endpoints.
pub struct DesktopMcp {
    /// The containers whose declarations are watched now, by record id.
    watchers: Mutex<HashSet<String>>,
    /// Servers whose tools are being read, by record key.
    fetching: Mutex<HashSet<String>>,
    /// Servers reading their tools again because they said they changed, by record key, and
    /// whether they said so once more meanwhile.
    rereads: Mutex<HashMap<String, bool>>,
    /// The connections endpoints hold to desktop servers.
    pool: client::Pool,
    /// The end of each server's stderr, by record key.
    logs: Mutex<HashMap<String, String>>,
    /// The names of endpoints whose tool list changed, for the agents connected to them.
    changes: tokio::sync::broadcast::Sender<String>,
    /// Wakes the reporters of the running desktops when a server or an endpoint changed.
    reports: tokio::sync::watch::Sender<()>,
    /// The MCP service of each endpoint that was reached, with what ends its sessions.
    services: Mutex<HashMap<String, gateway::Service>>,
    /// The inspectors that may be open at once.
    inspectors: Arc<tokio::sync::Semaphore>,
}
impl Default for DesktopMcp {
    fn default() -> Self {
        Self {
            watchers: Mutex::default(),
            fetching: Mutex::default(),
            rereads: Mutex::default(),
            pool: client::Pool::default(),
            logs: Mutex::default(),
            changes: tokio::sync::broadcast::channel(64).0,
            reports: tokio::sync::watch::channel(()).0,
            services: Mutex::default(),
            inspectors: Arc::new(tokio::sync::Semaphore::new(inspect::INSPECTORS)),
        }
    }
}

/// Forget a deleted container: its server records and connections, the endpoint created with
/// it, and its grants on other endpoints.
pub(crate) fn forget_container(app: &App, container_id: &str) -> anyhow::Result<()> {
    for record in app
        .db
        .list::<registry::ServerRecord>(registry::SERVERS)?
        .into_iter()
        .filter(|record| record.container_id == container_id)
    {
        app.db.remove(
            registry::SERVERS,
            &registry::ServerRecord::key(container_id, &record.name),
        )?;
    }
    app.desktop_mcp
        .pool
        .retain(|(_, container, _)| container != container_id);
    endpoints::forget_container(app, container_id)
}

#[cfg(test)]
#[path = "desktop_mcp_tests.rs"]
mod tests;
