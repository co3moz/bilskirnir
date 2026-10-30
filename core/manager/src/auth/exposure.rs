//! Whether the manager answers without a credential beyond this computer. Compose publishes its
//! port on `BIND_ADDRESS`, which the manager reads back from Docker as its own port bindings. On
//! an address other than loopback, every device that reaches it is an administrator while sign-in
//! is off (`AUTH_MODE=none`), and every agent is one while the root `/mcp` takes no token
//! (`MCP_AUTH_MODE=none`). So is every desktop with internet access: it reaches the host's
//! published port as a device on the network does, through the host, where `interfaces.rs` cannot
//! tell it from the administrator's own browser. The manager says so at startup and on the
//! Management page rather than refusing to start, since an installation set up that way keeps
//! working across an update.
use crate::{App, config::Config};
use serde_json::{Value, json};
use std::{net::IpAddr, sync::Arc};

/// What answers without a credential: the dashboard with the API, and the root `/mcp` with the
/// MCP endpoints.
fn open(config: &Config) -> Vec<&'static str> {
    let mut open = Vec::new();
    if !config.auth {
        open.push("dashboard");
    }
    if config.mcp_token.is_none() && config.root_mcp_enabled {
        open.push("mcp");
    }
    open
}

/// Where Docker publishes the ports of the container `info` describes beyond loopback, as
/// `ADDRESS:PORT`, with `*` for every address.
fn published(info: &Value) -> Vec<String> {
    let mut published = info["NetworkSettings"]["Ports"]
        .as_object()
        .into_iter()
        .flat_map(|ports| ports.values())
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(|binding| {
            let host = binding["HostIp"].as_str().unwrap_or_default();
            let port = binding["HostPort"].as_str()?;
            match host.parse::<IpAddr>() {
                Ok(address) if address.is_loopback() => None,
                Ok(address) if address.is_unspecified() => Some(format!("*:{port}")),
                Ok(IpAddr::V6(address)) => Some(format!("[{address}]:{port}")),
                Ok(address) => Some(format!("{address}:{port}")),
                Err(_) if host.is_empty() => Some(format!("*:{port}")),
                Err(_) => Some(format!("{host}:{port}")),
            }
        })
        .collect::<Vec<_>>();
    published.sort();
    published.dedup();
    published
}

/// What answers without a credential and where the manager is published beyond loopback, for the
/// Management page; `None` while either list is empty or the manager is not a container Docker
/// can describe.
pub(crate) async fn exposure(app: &App) -> Option<Value> {
    let open = open(&app.config);
    if open.is_empty() {
        return None;
    }
    let manager = app.config.manager.as_deref()?;
    let info = app.inspect("container", manager).await.ok()?;
    let published = published(&info);
    if published.is_empty() {
        return None;
    }
    Some(json!({"open": open, "published": published}))
}

/// What the startup log says about an [`exposure`].
fn warning(exposure: &Value) -> String {
    let open = |part: &str| {
        exposure["open"]
            .as_array()
            .is_some_and(|open| open.iter().any(|value| value == part))
    };
    let what = match (open("dashboard"), open("mcp")) {
        (true, true) => {
            "Sign-in is off (AUTH_MODE=none) and /mcp takes no token (MCP_AUTH_MODE=none)"
        }
        (true, false) => "Sign-in is off (AUTH_MODE=none)",
        _ => "/mcp takes no token (MCP_AUTH_MODE=none)",
    };
    let published = exposure["published"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{what} while the manager is published on {published}: every device that reaches that address, and every desktop with internet access, controls every container. Set ADMIN_PASSWORD and an MCP token, or BIND_ADDRESS=127.0.0.1."
    )
}

/// Log an [`exposure`] once, when the manager starts.
pub(crate) fn warn(app: Arc<App>) {
    tokio::spawn(async move {
        if let Some(exposure) = exposure(&app).await {
            tracing::warn!("{}", warning(&exposure));
        }
    });
}

#[cfg(test)]
#[path = "exposure_tests.rs"]
mod tests;
