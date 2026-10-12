use anyhow::{Result, bail};
use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
use std::{env, path::PathBuf};

pub struct Config {
    pub listen: String,
    pub data: PathBuf,
    pub web: PathBuf,
    pub docker_bin: String,
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
    pub allowed_hosts: Vec<String>,
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
        let cfg = Self {
            listen: env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            data: env::var("DATA_DIR")
                .unwrap_or_else(|_| "data".into())
                .into(),
            web: env::var("WEB_DIR")
                .unwrap_or_else(|_| "web/dist".into())
                .into(),
            docker_bin: env::var("DOCKER_BIN").unwrap_or_else(|_| "docker".into()),
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
            allowed_hosts: env::var("ALLOWED_HOSTS")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .collect(),
        };
        super::model::image_ref(&cfg.base_image)?;
        if cfg.auth && (cfg.username.trim().is_empty() || cfg.username.len() > 256) {
            bail!("ADMIN_USERNAME must not be empty and must be at most 256 bytes");
        }
        Ok(cfg)
    }
}
