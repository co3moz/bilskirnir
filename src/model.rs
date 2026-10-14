use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

pub const LABEL: &str = "io.wsl-desktop.managed";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mount {
    pub volume: String,
    pub target: String,
    #[serde(default)]
    pub readonly: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentSpec {
    pub name: String,
    pub image: String,
    #[serde(default = "resolution")]
    pub resolution: String,
    #[serde(default)]
    pub gpu: bool,
    #[serde(default = "yes")]
    pub internet: bool,
    #[serde(default)]
    pub audio: bool,
    #[serde(default)]
    pub microphone: bool,
    #[serde(default = "yes")]
    pub sudo: bool,
    /// Attach /dev/uinput so a virtual controller can be created inside the desktop.
    #[serde(default)]
    pub gamepad: bool,
    pub cpus: Option<f64>,
    pub memory_mb: Option<u64>,
    #[serde(default = "shm")]
    pub shm_mb: u64,
    #[serde(default)]
    pub mounts: Vec<Mount>,
}
fn resolution() -> String {
    "1920x1080".into()
}
fn yes() -> bool {
    true
}
fn shm() -> u64 {
    512
}

impl EnvironmentSpec {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() || self.name.chars().count() > 100 {
            bail!("Environment name must be 1–100 characters");
        }
        image_ref(&self.image)?;
        let (w, h) = self
            .resolution
            .split_once('x')
            .ok_or_else(|| anyhow::anyhow!("Resolution must use WIDTHxHEIGHT"))?;
        let (w, h) = (w.parse::<u32>()?, h.parse::<u32>()?);
        if !(320..=7680).contains(&w) || !(240..=4320).contains(&h) {
            bail!("Resolution must be between 320x240 and 7680x4320");
        }
        if let Some(c) = self.cpus
            && (!c.is_finite() || c <= 0.0)
        {
            bail!("CPU limit must be positive");
        }
        if let Some(m) = self.memory_mb
            && (!(256..=u64::MAX / 1_048_576).contains(&m))
        {
            bail!("Memory must be at least 256 MB");
        }
        if self.shm_mb < 64 || self.shm_mb > u64::MAX / 1_048_576 {
            bail!("Shared memory must be at least 64 MB");
        }
        if self.microphone && !self.audio {
            bail!("Enable audio to use the microphone");
        }
        let mut targets = std::collections::HashSet::new();
        for mount in &self.mounts {
            volume_name(&mount.volume)?;
            if !mount.target.starts_with('/')
                || mount.target == "/"
                || mount.target.contains([',', '\n', '\r', '\0'])
                || mount.target[1..]
                    .split('/')
                    .any(|s| s.is_empty() || s == "." || s == "..")
            {
                bail!("Invalid volume target");
            }
            if !targets.insert(&mount.target) {
                bail!("Volume targets must be unique");
            }
            if [
                "/proc",
                "/sys",
                "/dev",
                "/run",
                "/tmp",
                "/usr",
                "/etc",
                "/bin",
                "/sbin",
                "/lib",
                "/lib64",
                "/var/run",
                "/var/lock",
                "/opt/VirtualGL",
            ]
            .iter()
            .any(|p| mount.target == *p || mount.target.starts_with(&format!("{p}/")))
            {
                bail!("Volume target conflicts with desktop infrastructure");
            }
            if mount.target == "/opt" || mount.target == "/var" || mount.target == "/home" {
                bail!("Volume target conflicts with desktop infrastructure");
            }
            if mount.readonly
                && (mount.target == "/home/desktop" || mount.target.starts_with("/home/desktop/."))
            {
                bail!("Desktop home and configuration paths must be writable");
            }
        }
        Ok(())
    }
}
pub fn image_ref(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 255
        || value.starts_with('-')
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._:@-".contains(&b))
    {
        bail!("Invalid Docker image reference");
    }
    Ok(())
}
pub fn volume_name(value: &str) -> Result<()> {
    if value.len() < 2
        || value.len() > 128
        || !value.as_bytes()[0].is_ascii_alphanumeric()
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    {
        bail!("Invalid volume name");
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Environment {
    pub id: String,
    pub container: String,
    pub private_network: String,
    pub egress_network: String,
    pub source_image_id: String,
    pub created_at: u64,
    pub spec: EnvironmentSpec,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub desktop: Option<crate::profiles::DesktopInfo>,
    /// Set once a settings change recreated the container: the container runs this commit of
    /// its earlier files, while `source_image_id` stays the image the desktop started from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_image: Option<WorkingImage>,
}
impl Environment {
    /// The image the container is created from.
    pub fn container_image(&self) -> &str {
        self.working_image
            .as_ref()
            .map_or(&self.source_image_id, |image| &image.image_id)
    }
}
/// A commit that carries a container's files over to its replacement.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkingImage {
    pub image_id: String,
    pub reference: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageRecord {
    pub id: String,
    pub name: String,
    pub image_id: String,
    pub reference: String,
    pub parent_image_id: Option<String>,
    pub source_environment: Option<String>,
    pub kind: String,
    pub created_at: u64,
    pub description: String,
    #[serde(default)]
    pub desktop: Option<crate::profiles::DesktopInfo>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Application {
    pub id: String,
    pub environment_id: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub gpu: bool,
    pub created_at: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Terminal {
    pub id: String,
    pub environment_id: String,
    pub root: bool,
    pub cwd: String,
    pub created_at: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recording {
    pub id: String,
    pub environment_id: String,
    pub name: String,
    pub fps: u8,
    pub created_at: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub resource: Option<String>,
    pub status: String,
    pub created_at: u64,
    pub finished_at: Option<u64>,
    pub log: String,
    pub result: Option<serde_json::Value>,
    /// Measured progress of the running step, such as an image download.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<serde_json::Value>,
}
impl Job {
    pub fn trim_log(&mut self) {
        const LIMIT: usize = 262_144;
        if self.log.len() > LIMIT {
            let mut at = self.log.len() - LIMIT;
            while !self.log.is_char_boundary(at) {
                at += 1;
            }
            self.log.drain(..at);
        }
    }
}
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
