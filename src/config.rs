use anyhow::{Result, bail};
use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
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
    pub mcp_enabled: bool,
    /// The bearer token `/mcp` requires, or `None` when `MCP_AUTH_MODE=none` turns the check off.
    pub mcp_token: Option<String>,
}
fn flag(key: &str, default: bool) -> Result<bool> {
    match env::var(key).as_deref() {
        Ok("true" | "1") => Ok(true),
        Ok("false" | "0") => Ok(false),
        Err(_) => Ok(default),
        _ => bail!("{key}: must be true or false"),
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
        let mcp_enabled = flag("MCP_ENABLED", false)?;
        let mcp_token = mcp_token(
            mcp_enabled,
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
                .unwrap_or_else(|_| crate::profiles::DEFAULT_IMAGE.into()),
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
            mcp_enabled,
            mcp_token,
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
}

/// The socket must reach the daemon the CLI talks to, so it follows the CLI's own `DOCKER_HOST`.
pub(crate) fn docker_socket(host: Option<&str>) -> Option<PathBuf> {
    match host.filter(|host| !host.is_empty()) {
        None => Some("/var/run/docker.sock".into()),
        Some(host) => host.strip_prefix("unix://").map(PathBuf::from),
    }
}

/// An unset mode keeps the token requirement, so only an explicit `none` opens the endpoint.
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
            "MCP_TOKEN must contain 1 to 256 visible ASCII characters without spaces when MCP_ENABLED=true (or use MCP_AUTH_MODE=none)"
        );
    }
    Ok(())
}
