//! The proxy a desktop's programs go through: a container's `proxy` setting chooses the
//! installation's (`PROXY_URL`), one of its own, or none. Programs see only
//! `HTTP_PROXY=http://127.0.0.1:31280`, which [`environment`] gives every container with a proxy;
//! a small proxy inside the desktop (`helpers/outbound_proxy.py`), run as nobody, goes on with the
//! user name and password that only its memory holds, or straight to a bypassed host's address.
//!
//! The password of a container's own proxy lives in a record kind of its own, never in the
//! container record, a route, a job log or a commit, and backups carry it only with the secret
//! values ([`credentials`]). [`supervise`] starts each running desktop's proxy and keeps it in line
//! with the settings, so every change but switching the proxy on or off applies without a restart.
mod choice;
mod credentials;
mod supervise;

pub(crate) use choice::{InstallationProxy, ProxyChoice, installation, password};
pub(crate) use credentials::{CREDENTIALS, Credentials, PasswordChange};
pub(crate) use supervise::spawn;

use crate::{config::Config, model::ContainerSpec};
use blake2::{Blake2s256, Digest};
use serde::Serialize;
use std::{collections::HashMap, sync::Mutex};

/// Where programs in a desktop with a proxy reach it.
pub(crate) const PORT: u16 = 31280;
/// What programs reach without the proxy: the desktop itself.
const NO_PROXY: &str = "localhost,127.0.0.1,::1";

/// The supervisor's wake-up and what it last failed at, per container, so a lasting failure is
/// logged once.
#[derive(Default)]
pub(crate) struct Outbound {
    wake: tokio::sync::Notify,
    failures: Mutex<HashMap<String, String>>,
}

impl Outbound {
    /// Bring the running desktops' proxies in line now, after a start or a settings change.
    pub(crate) fn wake(&self) {
        self.wake.notify_one();
    }
}

/// The proxy variables every container gets. Without a proxy they are empty, so a proxy that a
/// commit kept in an image, such as a snapshot's, does not outlive the setting.
pub(crate) fn environment(spec: &ContainerSpec) -> [(&'static str, String); 6] {
    let (proxy, direct) = if spec.proxy.is_some() {
        (format!("http://127.0.0.1:{PORT}"), NO_PROXY.to_owned())
    } else {
        (String::new(), String::new())
    };
    [
        ("HTTP_PROXY", proxy.clone()),
        ("HTTPS_PROXY", proxy.clone()),
        ("http_proxy", proxy.clone()),
        ("https_proxy", proxy),
        ("NO_PROXY", direct.clone()),
        ("no_proxy", direct),
    ]
}

/// The proxy the desktop's own proxy goes on to, with its credentials.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct Upstream {
    pub scheme: choice::Scheme,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// What the desktop's proxy is given: the proxy to go on to, `None` to go straight to every
/// address, the hosts that always go straight, and a version that tells whether it has them.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct Settings {
    pub version: String,
    pub upstream: Option<Upstream>,
    pub bypass: Vec<String>,
}

/// The settings of a container's desktop proxy, or `None` for a container without a proxy. The
/// installation's proxy is read as it is now, so a container that follows it gets a change of
/// `PROXY_URL` once the manager starts again, and goes straight to every address without one.
pub(crate) fn settings(
    config: &Config,
    spec: &ContainerSpec,
    password: Option<String>,
) -> Option<Settings> {
    let (upstream, bypass) = match spec.proxy.as_ref()? {
        ProxyChoice::Installation => match &config.proxy {
            None => (None, Vec::new()),
            Some(proxy) => (
                Some(Upstream {
                    scheme: proxy.address.scheme,
                    host: proxy.address.host.clone(),
                    port: proxy.address.port,
                    username: proxy.username.clone(),
                    password: proxy.password.clone(),
                }),
                proxy.bypass.clone(),
            ),
        },
        ProxyChoice::Custom {
            url,
            username,
            bypass,
            ..
        } => {
            let address = choice::address(url).ok()?;
            (
                Some(Upstream {
                    scheme: address.scheme,
                    host: address.host,
                    port: address.port,
                    username: username.clone(),
                    password,
                }),
                bypass.clone(),
            )
        }
    };
    let digest = Blake2s256::digest(serde_json::to_vec(&(&upstream, &bypass)).ok()?);
    Some(Settings {
        version: digest.iter().map(|b| format!("{b:02x}")).collect(),
        upstream,
        bypass,
    })
}

#[cfg(test)]
#[path = "outbound_tests.rs"]
mod tests;
