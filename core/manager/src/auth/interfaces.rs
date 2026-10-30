//! The manager's own addresses, by the network each is on. The manager shares a private network
//! with every desktop, to reach its VNC server and its proxied ports, and listens on every address
//! it has, so a program in a desktop could otherwise reach the dashboard, the API and MCP on that
//! network, whatever `BIND_ADDRESS` publishes and with sign-in off as an administrator. A desktop
//! never needs them: the manager reaches it, through `docker exec` and its private address, never
//! the other way round. So a connection that arrives on the manager's address in a network Thoric
//! made is refused, and one on an address Docker does not name for the manager as well, such as a
//! link-local one. Loopback and the networks Compose or the administrator attached the manager to
//! are answered.
//!
//! An address is looked up in Docker the first time a connection reaches it, so a network the
//! manager joined a moment ago is never answered before it is known to be a desktop's.
use crate::{App, model::LABEL};
use anyhow::{Context, Result};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
    sync::RwLock,
    time::{Duration, Instant},
};

/// Docker is asked about an unknown address at most this often, so a desktop that reaches the
/// manager on an address Docker never names does not run Docker on every request.
const LOOKUP_EVERY: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(crate) struct Interfaces {
    /// Whether a connection on each of the manager's addresses is answered, as Docker last named
    /// them.
    known: RwLock<HashMap<IpAddr, bool>>,
    /// When Docker last named them, held while it is asked so lookups do not pile up.
    looked_up: tokio::sync::Mutex<Option<Instant>>,
}
impl Interfaces {
    /// Whether a connection that reached the manager on its address `local` is answered. A
    /// manager that is not a container, as in development, shares no network with a desktop.
    pub(crate) async fn answers(&self, app: &App, local: IpAddr) -> Result<bool> {
        let local = local.to_canonical();
        if local.is_loopback() {
            return Ok(true);
        }
        let Some(manager) = &app.config.manager else {
            return Ok(true);
        };
        if let Some(answers) = self.known(local) {
            return Ok(answers);
        }
        let mut looked_up = self.looked_up.lock().await;
        // Another connection may have had it looked up while this one waited.
        if let Some(answers) = self.known(local) {
            return Ok(answers);
        }
        if looked_up.is_some_and(|at| at.elapsed() < LOOKUP_EVERY) {
            return Ok(false);
        }
        let addresses = lookup(app, manager).await?;
        *looked_up = Some(Instant::now());
        *self.known.write().unwrap() = addresses;
        Ok(self.known(local).unwrap_or(false))
    }
    fn known(&self, local: IpAddr) -> Option<bool> {
        self.known.read().unwrap().get(&local).copied()
    }
}

/// The manager's addresses as Docker names them, and whether each is answered.
async fn lookup(app: &App, manager: &str) -> Result<HashMap<IpAddr, bool>> {
    let container = app
        .inspect("container", manager)
        .await
        .context("Could not read the manager's networks")?;
    let made = app
        .docker(&[
            "network",
            "ls",
            "--quiet",
            "--no-trunc",
            "--filter",
            &format!("label={LABEL}=true"),
        ])
        .await
        .context("Could not read the manager's networks")?;
    let made = String::from_utf8_lossy(&made);
    Ok(addresses(&container, &made.lines().collect()))
}

/// Each address of the manager's container, refused on a network of `made`, the IDs of the
/// networks Thoric made, and answered on any other.
fn addresses(container: &Value, made: &HashSet<&str>) -> HashMap<IpAddr, bool> {
    let mut addresses = HashMap::new();
    let Some(networks) = container["NetworkSettings"]["Networks"].as_object() else {
        return addresses;
    };
    for network in networks.values() {
        let answers = !network["NetworkID"]
            .as_str()
            .is_some_and(|id| made.contains(id));
        for key in ["IPAddress", "GlobalIPv6Address"] {
            let Some(address) = network[key].as_str().and_then(|a| a.parse::<IpAddr>().ok()) else {
                continue;
            };
            // Should two networks ever give the manager one address, a desktop's wins.
            *addresses.entry(address).or_insert(answers) &= answers;
        }
    }
    addresses
}

#[cfg(test)]
#[path = "interfaces_tests.rs"]
mod tests;
