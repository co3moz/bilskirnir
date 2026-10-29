//! The address a sign-in comes from behind the HTTPS proxy Compose runs beside the manager.
//!
//! Through the bundled Caddy (`deploy/Caddyfile`, the `https` profile), every browser reaches the
//! manager from Caddy's address, so their failed sign-ins would share one count. Caddy names the
//! address it saw in `X-Forwarded-For`, and the manager believes that header only from Caddy's own
//! addresses, which it reads from Docker: those of the running containers of its own Compose
//! project's `https` service. Docker gives each address to one container, desktops do not share
//! Caddy's network, and a LAN client reaches the manager from its own address or the network's
//! gateway, so nobody else can send from one of them. Anyone else's header is ignored and the
//! connection's address counts.
use crate::App;
use anyhow::Result;
use axum::http::HeaderMap;
use serde_json::Value;
use std::{
    net::{IpAddr, SocketAddr},
    sync::{Arc, RwLock},
    time::Duration,
};

/// The Compose labels that name a container's project and service.
const PROJECT_LABEL: &str = "com.docker.compose.project";
const SERVICE_LABEL: &str = "com.docker.compose.service";
/// The Compose service of the bundled HTTPS proxy.
const HTTPS_SERVICE: &str = "https";
/// How often the proxy's addresses are read again, since Compose may recreate it.
const REFRESH: Duration = Duration::from_secs(20);

/// The addresses of the bundled HTTPS proxy, whose `X-Forwarded-For` names the client.
#[derive(Default)]
pub struct HttpsProxy(RwLock<Vec<IpAddr>>);

impl HttpsProxy {
    /// Replace the addresses, and say whether they changed.
    pub(crate) fn set(&self, mut addresses: Vec<IpAddr>) -> bool {
        addresses.sort();
        addresses.dedup();
        let mut current = self.0.write().unwrap();
        if *current == addresses {
            return false;
        }
        *current = addresses;
        true
    }
    fn contains(&self, address: IpAddr) -> bool {
        self.0.read().unwrap().contains(&address)
    }
}

/// The client a request comes from: the last address of `X-Forwarded-For`, the one the proxy
/// added, when the connection comes from the HTTPS proxy, and otherwise the connection's own.
pub(crate) fn client(
    peer: Option<IpAddr>,
    headers: &HeaderMap,
    proxy: &HttpsProxy,
) -> Option<IpAddr> {
    let peer = peer.map(|address| address.to_canonical());
    match peer {
        Some(address) if proxy.contains(address) => forwarded(headers).or(peer),
        _ => peer,
    }
}

fn forwarded(headers: &HeaderMap) -> Option<IpAddr> {
    let header = headers.get_all("x-forwarded-for").iter().next_back()?;
    let entry = header.to_str().ok()?.rsplit(',').next()?.trim();
    entry
        .parse::<IpAddr>()
        .or_else(|_| entry.parse::<SocketAddr>().map(|address| address.ip()))
        .ok()
        .map(|address| address.to_canonical())
}

/// Every address a container has on its networks, from its `docker inspect`.
fn addresses(info: &Value) -> Vec<IpAddr> {
    let Some(networks) = info["NetworkSettings"]["Networks"].as_object() else {
        return Vec::new();
    };
    networks
        .values()
        .flat_map(|network| [&network["IPAddress"], &network["GlobalIPv6Address"]])
        .filter_map(|address| address.as_str()?.parse().ok())
        .collect()
}

/// The addresses of the running `https` service of Compose project `project`.
async fn https_proxy(app: &App, project: &str) -> Result<Vec<IpAddr>> {
    let listed = app
        .docker_args(&[
            "ps".into(),
            "--quiet".into(),
            "--no-trunc".into(),
            "--filter".into(),
            format!("label={PROJECT_LABEL}={project}"),
            "--filter".into(),
            format!("label={SERVICE_LABEL}={HTTPS_SERVICE}"),
        ])
        .await?;
    let mut args = vec!["container".to_owned(), "inspect".to_owned()];
    args.extend(
        String::from_utf8_lossy(&listed)
            .split_whitespace()
            .map(str::to_owned),
    );
    if args.len() == 2 {
        return Ok(Vec::new());
    }
    let infos: Vec<Value> = serde_json::from_slice(&app.docker_args(&args).await?)?;
    Ok(infos.iter().flat_map(addresses).collect())
}

/// Follow the HTTPS proxy's addresses while the manager runs in a Compose project. A failure to
/// read them trusts no proxy until the next reading, and is logged once until it changes.
pub(crate) fn spawn(app: Arc<App>) {
    let Some(manager) = app.config.manager.clone() else {
        return;
    };
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(REFRESH);
        let mut failure = None;
        // A container's labels never change, so the manager's project is read once.
        let project = loop {
            tick.tick().await;
            match app.inspect("container", &manager).await {
                Ok(info) => {
                    break info["Config"]["Labels"][PROJECT_LABEL]
                        .as_str()
                        .map(str::to_owned);
                }
                Err(error) => report(&mut failure, &error),
            }
        };
        let Some(project) = project else {
            return;
        };
        loop {
            match https_proxy(&app, &project).await {
                Ok(addresses) => {
                    if failure.take().is_some() {
                        tracing::info!("Docker names the HTTPS proxy's address again");
                    }
                    let shown = addresses.iter().map(IpAddr::to_string).collect::<Vec<_>>();
                    if app.https_proxy.set(addresses) {
                        if shown.is_empty() {
                            tracing::info!(
                                "No HTTPS proxy runs: sign-ins count by the connection's address"
                            );
                        } else {
                            tracing::info!(
                                "Sign-ins through the HTTPS proxy at {} count by the address it forwards",
                                shown.join(", ")
                            );
                        }
                    }
                }
                Err(error) => {
                    app.https_proxy.set(Vec::new());
                    report(&mut failure, &error);
                }
            }
            tick.tick().await;
        }
    });
}

fn report(failure: &mut Option<String>, error: &anyhow::Error) {
    let error = format!("{error:#}");
    if failure.as_ref() != Some(&error) {
        tracing::warn!(
            "Could not read the HTTPS proxy's address from Docker, so sign-ins through it count by its address: {error}"
        );
        *failure = Some(error);
    }
}

#[cfg(test)]
#[path = "forwarded_tests.rs"]
mod tests;
