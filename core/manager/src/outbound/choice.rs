//! What a container's `proxy` setting and the installation's `PROXY_*` settings hold, and the
//! rules their values follow: an address is `scheme://host:port` alone, the credentials go beside
//! it, and the bypass list names hosts, domains and networks.
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv6Addr};

/// A container's proxy, beside `None` for none.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ProxyChoice {
    /// The installation's proxy, `PROXY_URL`, as it is set now.
    Installation,
    /// A proxy of the container's own.
    Custom {
        url: String,
        #[serde(default)]
        username: Option<String>,
        #[serde(default)]
        bypass: Vec<String>,
        /// Whether a password is kept for it; the manager sets this, never a request.
        #[serde(default)]
        password: bool,
    },
}

impl ProxyChoice {
    pub fn validate(&self) -> Result<()> {
        if let Self::Custom {
            url,
            username,
            bypass,
            ..
        } = self
        {
            address(url)?;
            if let Some(name) = username {
                self::username(name)?;
            }
            bypass_list(bypass)?;
        }
        Ok(())
    }

    /// The host of a container's own proxy, which its password belongs to.
    pub(crate) fn host(&self) -> Option<String> {
        match self {
            Self::Custom { url, .. } => address(url).ok().map(|address| address.host),
            Self::Installation => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    Http,
    Https,
    Socks5,
}

/// Where a proxy listens.
#[derive(Clone, Debug, PartialEq)]
pub struct Address {
    pub scheme: Scheme,
    pub host: String,
    pub port: u16,
}

/// A proxy's address, `http://`, `https://` or `socks5://` with a host and a port and nothing
/// else. Credentials have fields of their own, so an address can be shown.
pub(crate) fn address(url: &str) -> Result<Address> {
    let wrong = || {
        anyhow::anyhow!(
            "A proxy address is http://, https:// or socks5:// with a host and a port, such as http://proxy.example:3128"
        )
    };
    let (scheme, rest) = url.split_once("://").ok_or_else(wrong)?;
    let scheme = match scheme.to_ascii_lowercase().as_str() {
        "http" => Scheme::Http,
        "https" => Scheme::Https,
        "socks5" => Scheme::Socks5,
        _ => return Err(wrong()),
    };
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    if rest.contains('@') {
        bail!(
            "A proxy address carries no user name or password; give them in username and proxy_password (PROXY_USERNAME and PROXY_PASSWORD for the installation)"
        );
    }
    if rest.contains(['/', '?', '#']) {
        return Err(wrong());
    }
    let (host, port) = if let Some(bracketed) = rest.strip_prefix('[') {
        let (host, port) = bracketed.split_once("]:").ok_or_else(wrong)?;
        host.parse::<Ipv6Addr>().map_err(|_| wrong())?;
        (host, port)
    } else {
        rest.rsplit_once(':').ok_or_else(wrong)?
    };
    let port = port.parse::<u16>().ok().filter(|port| *port > 0);
    let (Some(port), true) = (port, host_name(host) || host.parse::<IpAddr>().is_ok()) else {
        return Err(wrong());
    };
    Ok(Address {
        scheme,
        host: host.to_ascii_lowercase(),
        port,
    })
}

fn host_name(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 253
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
}

/// A user name a proxy takes: at most 255 bytes, as SOCKS5 carries it, and without the colon
/// that ends it in an HTTP proxy's Basic credentials.
pub(crate) fn username(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 255
        || value.contains(':')
        || value.chars().any(char::is_control)
    {
        bail!("A proxy user name has 1 to 255 bytes, without colons or control characters");
    }
    Ok(())
}

pub(crate) fn password(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        bail!("A proxy password has 1 to 255 bytes, without control characters");
    }
    Ok(())
}

/// Hosts that go straight to their address: `*` for all of them, a name with the names below it
/// (`example.com`, `.example.com` or `*.example.com`), an address or a network such as
/// `10.0.0.0/8`.
pub(crate) fn bypass_list(entries: &[String]) -> Result<()> {
    if entries.len() > 64 {
        bail!("A proxy bypass list has at most 64 entries");
    }
    for entry in entries {
        let domain = entry.trim_start_matches("*.").trim_start_matches('.');
        let network = match entry.split_once('/') {
            Some((address, prefix)) => address.parse::<IpAddr>().is_ok_and(|address| {
                let most = if address.is_ipv4() { 32 } else { 128 };
                prefix.parse::<u8>().is_ok_and(|prefix| prefix <= most)
            }),
            None => entry.parse::<IpAddr>().is_ok(),
        };
        if !(entry == "*" || network || host_name(domain)) {
            bail!(
                "{entry:?} is not a host, a domain such as .example.com, an address or a network such as 10.0.0.0/8"
            );
        }
    }
    Ok(())
}

/// The installation's proxy, which a container chooses with `{"mode": "installation"}`.
#[derive(Clone, Debug, PartialEq)]
pub struct InstallationProxy {
    pub url: String,
    pub address: Address,
    pub username: Option<String>,
    pub password: Option<String>,
    pub bypass: Vec<String>,
}

/// Read `PROXY_URL`, `PROXY_USERNAME`, `PROXY_PASSWORD` and `PROXY_BYPASS`, a comma-separated
/// list. Without `PROXY_URL` the installation has no proxy, and the others are only noted.
pub(crate) fn installation(
    var: &dyn Fn(&str) -> Option<String>,
    notices: &mut Vec<String>,
) -> Result<Option<InstallationProxy>> {
    let set = |key: &str| var(key).filter(|value| !value.is_empty());
    let Some(url) = set("PROXY_URL") else {
        for key in ["PROXY_USERNAME", "PROXY_PASSWORD", "PROXY_BYPASS"] {
            if set(key).is_some() {
                notices.push(format!("{key} is set without PROXY_URL, so it has no use."));
            }
        }
        return Ok(None);
    };
    let named =
        |key: &str, result: Result<()>| result.map_err(|error| anyhow::anyhow!("{key}: {error:#}"));
    let address = address(&url).map_err(|error| anyhow::anyhow!("PROXY_URL: {error:#}"))?;
    let username = set("PROXY_USERNAME");
    if let Some(name) = &username {
        named("PROXY_USERNAME", self::username(name))?;
    }
    let password = set("PROXY_PASSWORD");
    if let Some(value) = &password {
        named("PROXY_PASSWORD", self::password(value))?;
        if username.is_none() {
            bail!("PROXY_PASSWORD needs PROXY_USERNAME");
        }
    }
    let bypass: Vec<String> = set("PROXY_BYPASS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect();
    named("PROXY_BYPASS", bypass_list(&bypass))?;
    Ok(Some(InstallationProxy {
        url,
        address,
        username,
        password,
        bypass,
    }))
}
