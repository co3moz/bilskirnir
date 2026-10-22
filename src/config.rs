use crate::model::LABEL;
use anyhow::{Result, bail};
use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
use serde_json::{Value, json};
use std::{env, path::PathBuf};

pub struct Config {
    pub listen: String,
    pub data: PathBuf,
    pub web: PathBuf,
    pub docker_bin: String,
    /// The Engine API socket for pulls with byte progress; without one, pulls use the CLI.
    pub docker_socket: Option<PathBuf>,
    pub manager: Option<String>,
    pub base_image: String,
    pub desktop_context: PathBuf,
    pub auth: bool,
    pub username: String,
    pub password_hash: Option<String>,
    pub secure_cookie: bool,
    pub audio: bool,
    pub microphone: bool,
    pub wsl_gpu: bool,
    pub cuda: bool,
    pub adapter: String,
    /// Whether desktops may create user namespaces, which is how Chromium-based browsers and
    /// Electron applications sandbox their processes. See [`Config::seccomp_profile`].
    pub user_namespaces: bool,
    pub allowed_hosts: Vec<String>,
    pub upload_limit: u64,
    /// The largest unpacked image archive an import accepts, in bytes.
    pub archive_limit: u64,
    /// Whether the root `/mcp` answers: the administrator's agent channel to every container, the
    /// one not made on the dashboard. The MCP endpoints at `/mcp/<name>` answer either way.
    pub root_mcp_enabled: bool,
    /// The administrator's bearer token, which `/mcp` requires and which opens every MCP endpoint
    /// too, or `None` when `MCP_AUTH_MODE=none` turns the check off for both. Empty when no token
    /// is set, which opens nothing.
    pub mcp_token: Option<String>,
    /// The template list the templates page reads in the browser, or `None` to turn it off.
    pub templates_url: Option<String>,
    /// The label a volume carries for a request to list and choose it, `key` or `key=value`, or
    /// `None` for every volume. See [`volume_filter`].
    pub volume_filter: Option<String>,
    /// Whether the manager looks for newer releases, which the dashboard then offers.
    pub update_check: bool,
    /// Where the WSL package takes update requests from the dashboard; see [`crate::releases`].
    pub update_dir: PathBuf,
    /// What startup has to say about the settings it was given, such as a renamed variable that
    /// is still set; logged once the manager starts.
    pub notices: Vec<String>,
}
/// A variable a release renamed. An installation keeps its `.env` across upgrades, so the old name
/// keeps its meaning while the new one is not set, and startup says to rename it.
pub(crate) struct Renamed {
    pub old: &'static str,
    pub new: &'static str,
    /// The release that renamed it.
    pub release: &'static str,
}
pub(crate) const RENAMED: &[Renamed] = &[Renamed {
    old: "MCP_ENABLED",
    new: "ROOT_MCP_ENABLED",
    release: "0.7.0",
}];
/// The template list on the project's GitHub wiki.
pub(crate) const TEMPLATES_URL: &str =
    "https://raw.githubusercontent.com/wiki/co3moz/bilskirnir/templates/index.json";
fn flag(key: &str, default: bool) -> Result<bool> {
    flag_value(key, env::var(key).ok().as_deref(), default)
}
fn flag_value(key: &str, value: Option<&str>, default: bool) -> Result<bool> {
    match value {
        Some("true" | "1") => Ok(true),
        Some("false" | "0") => Ok(false),
        None => Ok(default),
        _ => bail!("{key}: must be true or false"),
    }
}
/// The value of a renamed setting under either of its names. Empty counts as not set, since
/// Compose passes the old name empty once `.env` no longer has it.
pub(crate) fn renamed_value(
    get: &dyn Fn(&str) -> Option<String>,
    renamed: &Renamed,
    notices: &mut Vec<String>,
) -> Result<Option<String>> {
    let set = |key: &str| get(key).filter(|value| !value.is_empty());
    let Renamed { old, new, release } = renamed;
    match (set(new), set(old)) {
        (new_value, None) => Ok(new_value),
        (None, Some(old_value)) => {
            notices.push(format!(
                "{old} was renamed {new} in Bilskirnir {release}. Its value still applies; rename it in .env."
            ));
            Ok(Some(old_value))
        }
        (Some(new_value), Some(old_value)) if new_value == old_value => {
            notices.push(format!(
                "{old} was renamed {new} in Bilskirnir {release}, and {new} applies. Remove {old} from .env."
            ));
            Ok(Some(new_value))
        }
        (Some(new_value), Some(old_value)) => bail!(
            "{old}={old_value} and {new}={new_value} disagree. {old} was renamed {new} in Bilskirnir {release}; remove {old} from .env"
        ),
    }
}
impl Config {
    pub fn load() -> Result<Self> {
        let mode = env::var("AUTH_MODE").unwrap_or_else(|_| "password".into());
        if mode != "password" && mode != "none" {
            bail!("AUTH_MODE must be password or none");
        }
        let auth = mode == "password";
        let password_hash = if auth {
            let password = env::var("ADMIN_PASSWORD").unwrap_or_default();
            if password.chars().count() < 8 || password.len() > 4096 {
                bail!(
                    "ADMIN_PASSWORD must have at least 8 characters and at most 4096 bytes (or use AUTH_MODE=none)"
                );
            }
            let salt = SaltString::encode_b64(uuid::Uuid::new_v4().as_bytes())
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            Some(
                Argon2::default()
                    .hash_password(password.as_bytes(), &salt)
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?
                    .to_string(),
            )
        } else {
            None
        };
        let audio = flag("AUDIO_ENABLED", true)?;
        let mut notices = Vec::new();
        let get = |key: &str| env::var(key).ok();
        let root_mcp = renamed_value(&get, &RENAMED[0], &mut notices)?;
        let root_mcp_enabled = flag_value("ROOT_MCP_ENABLED", root_mcp.as_deref(), false)?;
        let mcp_token = mcp_token(
            root_mcp_enabled,
            env::var("MCP_AUTH_MODE").ok().as_deref(),
            env::var("MCP_TOKEN").unwrap_or_default(),
        )?;
        let upload_mb = env::var("UPLOAD_MAX_MB")
            .unwrap_or_else(|_| "1024".into())
            .parse::<u64>()?;
        if !(1..=65536).contains(&upload_mb) {
            bail!("UPLOAD_MAX_MB must be between 1 and 65536");
        }
        let archive_mb = env::var("IMAGE_ARCHIVE_MAX_MB")
            .unwrap_or_else(|_| "65536".into())
            .parse::<u64>()?;
        if !(1..=1_048_576).contains(&archive_mb) {
            bail!("IMAGE_ARCHIVE_MAX_MB must be between 1 and 1048576");
        }
        let cfg = Self {
            listen: env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            data: env::var("DATA_DIR")
                .unwrap_or_else(|_| "data".into())
                .into(),
            web: env::var("WEB_DIR")
                .unwrap_or_else(|_| "web/dist".into())
                .into(),
            docker_bin: env::var("DOCKER_BIN").unwrap_or_else(|_| "docker".into()),
            docker_socket: docker_socket(env::var("DOCKER_HOST").ok().as_deref()),
            manager: env::var("MANAGER_CONTAINER").ok().filter(|s| !s.is_empty()),
            base_image: env::var("DESKTOP_IMAGE")
                .unwrap_or_else(|_| crate::catalog::DEFAULT_IMAGE.into()),
            desktop_context: env::var("DESKTOP_CONTEXT")
                .unwrap_or_else(|_| "desktop".into())
                .into(),
            username: env::var("ADMIN_USERNAME").unwrap_or_else(|_| "admin".into()),
            auth,
            password_hash,
            secure_cookie: flag("SECURE_COOKIE", false)?,
            audio,
            microphone: audio && flag("MICROPHONE_ENABLED", true)?,
            wsl_gpu: flag("WSL_GPU", true)?,
            cuda: flag("NVIDIA_COMPUTE", true)?,
            adapter: env::var("GPU_ADAPTER").unwrap_or_default(),
            user_namespaces: flag("DESKTOP_USER_NAMESPACES", true)?,
            allowed_hosts: env::var("ALLOWED_HOSTS")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect(),
            upload_limit: upload_mb * 1024 * 1024,
            archive_limit: archive_mb * 1024 * 1024,
            root_mcp_enabled,
            mcp_token,
            templates_url: templates_url(env::var("TEMPLATES_URL").ok().as_deref())?,
            volume_filter: volume_filter(env::var("VOLUME_FILTER").ok().as_deref())?,
            update_check: flag("UPDATE_CHECK", true)?,
            update_dir: crate::releases::UPDATE_DIR.into(),
            notices,
        };
        super::model::image_ref(&cfg.base_image)?;
        if let Some(profile) = cfg.seccomp_profile()
            && !profile.is_file()
        {
            bail!(
                "{} is missing; restore it or set DESKTOP_USER_NAMESPACES=false to give desktops Docker's default seccomp profile",
                profile.display()
            );
        }
        if cfg.auth && (cfg.username.trim().is_empty() || cfg.username.len() > 256) {
            bail!("ADMIN_USERNAME must not be empty and must be at most 256 bytes");
        }
        Ok(cfg)
    }

    /// The seccomp profile desktops are created with, or `None` for Docker's default.
    ///
    /// Docker's default profile refuses `clone` and `unshare` with namespace flags to a container
    /// without `CAP_SYS_ADMIN`. Chrome and Electron applications then cannot start their sandbox
    /// and abort unless they get `--no-sandbox`, which the desktop's menu entries and default web
    /// browser never pass. `desktop/seccomp.json` is Docker's default with one rule added
    /// that allows both calls; everything that needs `CAP_SYS_ADMIN`, such as `mount` and
    /// `setns`, stays refused. The manager image bundles it with the desktop build context.
    pub(crate) fn seccomp_profile(&self) -> Option<PathBuf> {
        self.user_namespaces
            .then(|| self.desktop_context.join("seccomp.json"))
    }

    /// The settings of `.env.example` that Compose passes to the manager, in the order the settings
    /// page lists them, each with the value the manager runs with rather than the text it was
    /// given: an empty `TEMPLATES_URL` shows the wiki's list, `ALLOWED_HOSTS` includes
    /// `DESKTOP_HOST`. A secret says only whether it is set; its value never leaves the manager.
    pub(crate) fn shown(&self) -> Vec<Value> {
        let value = |name: &str, value: Value| json!({"name": name, "value": value});
        let secret = |name: &str, set: bool| json!({"name": name, "secret": true, "set": set});
        let mib = |bytes: u64| json!(bytes / (1024 * 1024));
        let on = |enabled: bool, mode: &'static str| if enabled { mode } else { "none" };
        vec![
            value("AUTH_MODE", json!(on(self.auth, "password"))),
            value("ADMIN_USERNAME", json!(self.username)),
            secret("ADMIN_PASSWORD", self.password_hash.is_some()),
            value("SECURE_COOKIE", json!(self.secure_cookie)),
            value("ALLOWED_HOSTS", json!(self.allowed_hosts.join(","))),
            value("MANAGER_CONTAINER", json!(self.manager)),
            value("ROOT_MCP_ENABLED", json!(self.root_mcp_enabled)),
            value(
                "MCP_AUTH_MODE",
                json!(on(self.mcp_token.is_some(), "token")),
            ),
            secret(
                "MCP_TOKEN",
                self.mcp_token
                    .as_ref()
                    .is_some_and(|token| !token.is_empty()),
            ),
            value("DESKTOP_IMAGE", json!(self.base_image)),
            value("DESKTOP_USER_NAMESPACES", json!(self.user_namespaces)),
            value("WSL_GPU", json!(self.wsl_gpu)),
            value("NVIDIA_COMPUTE", json!(self.cuda)),
            value("GPU_ADAPTER", json!(self.adapter)),
            value("AUDIO_ENABLED", json!(self.audio)),
            value("MICROPHONE_ENABLED", json!(self.microphone)),
            value("UPLOAD_MAX_MB", mib(self.upload_limit)),
            value("IMAGE_ARCHIVE_MAX_MB", mib(self.archive_limit)),
            value(
                "TEMPLATES_URL",
                json!(self.templates_url.as_deref().unwrap_or("none")),
            ),
            value(
                "VOLUME_FILTER",
                json!(self.volume_filter.as_deref().unwrap_or("none")),
            ),
            value("UPDATE_CHECK", json!(self.update_check)),
        ]
    }

    /// The scheme and host the templates page reads from, which the interface's content security
    /// policy has to allow.
    pub(crate) fn templates_origin(&self) -> Option<&str> {
        let url = self.templates_url.as_deref()?;
        let host = url.find("://")? + 3;
        Some(&url[..url[host..].find('/').map_or(url.len(), |end| host + end)])
    }
}

/// `TEMPLATES_URL`: empty for the project wiki's list, `none` for no templates page, or the URL
/// of another list. The browser reads it, and the content security policy names its host, so it
/// is held to characters neither can misread.
pub(crate) fn templates_url(value: Option<&str>) -> Result<Option<String>> {
    let value = value.unwrap_or_default().trim();
    if value.is_empty() {
        return Ok(Some(TEMPLATES_URL.into()));
    }
    if value == "none" {
        return Ok(None);
    }
    let host = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .map(|rest| rest.split('/').next().unwrap_or_default());
    if value.len() > 2048
        || host.is_none_or(|host| {
            host.is_empty()
                || !host
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b".-:[]".contains(&b))
        })
        || !value
            .bytes()
            .all(|b| b.is_ascii_graphic() && !b"\"'`;,<>\\{}|^".contains(&b))
    {
        bail!("TEMPLATES_URL must be an http or https URL, empty for the project wiki, or none");
    }
    Ok(Some(value.into()))
}

/// `VOLUME_FILTER`: the label a volume needs to be listed and chosen, as `key` or `key=value`.
/// Empty keeps the label the manager puts on every volume it creates, so volumes made outside
/// Bilskirnir, the manager's own data volume among them, are out of reach; `none` lists every
/// volume. The manager gives the label to each volume it creates, so the label has to be one
/// `docker volume create --label` and Docker's `label` filter read the same way.
pub(crate) fn volume_filter(value: Option<&str>) -> Result<Option<String>> {
    let value = value.unwrap_or_default().trim();
    if value.is_empty() {
        return Ok(Some(format!("{LABEL}=true")));
    }
    if value == "none" {
        return Ok(None);
    }
    let (key, label) = value.split_once('=').unwrap_or((value, ""));
    if value.len() > 256
        || key.is_empty()
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b))
        || !label.bytes().all(|b| b.is_ascii_graphic() && b != b',')
    {
        bail!("VOLUME_FILTER must be a label as key or key=value, empty for {LABEL}=true, or none");
    }
    // The ownership label means the same on every volume the manager creates.
    if key == LABEL && !matches!(value.split_once('='), None | Some((_, "true"))) {
        bail!("VOLUME_FILTER can ask for {LABEL} only with the value true");
    }
    Ok(Some(value.into()))
}

/// The socket must reach the daemon the CLI talks to, so it follows the CLI's own `DOCKER_HOST`.
pub(crate) fn docker_socket(host: Option<&str>) -> Option<PathBuf> {
    match host.filter(|host| !host.is_empty()) {
        None => Some("/var/run/docker.sock".into()),
        Some(host) => host.strip_prefix("unix://").map(PathBuf::from),
    }
}

/// An unset mode keeps the token requirement, so only an explicit `none` opens `/mcp` and the MCP
/// endpoints without one. The token itself is required only while the root `/mcp` is on; without
/// it, an endpoint opens with its own token.
pub(crate) fn mcp_token(
    enabled: bool,
    mode: Option<&str>,
    token: String,
) -> Result<Option<String>> {
    match mode.unwrap_or("token") {
        "token" => {
            validate_mcp_token(enabled, &token)?;
            Ok(Some(token))
        }
        "none" => Ok(None),
        _ => bail!("MCP_AUTH_MODE must be token or none"),
    }
}

pub(crate) fn validate_mcp_token(enabled: bool, token: &str) -> Result<()> {
    if enabled
        && (!(1..=256).contains(&token.len()) || !token.bytes().all(|b| b.is_ascii_graphic()))
    {
        bail!(
            "MCP_TOKEN must contain 1 to 256 visible ASCII characters without spaces when ROOT_MCP_ENABLED=true (or use MCP_AUTH_MODE=none)"
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
