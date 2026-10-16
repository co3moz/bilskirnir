use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

pub const LABEL: &str = "io.wsl-desktop.managed";
/// The database kind of container records. They keep the name they were stored under before
/// environments were called containers, so an existing installation reads them unchanged.
pub const CONTAINERS: &str = "environment";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mount {
    pub volume: String,
    pub target: String,
    #[serde(default)]
    pub readonly: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContainerSpec {
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

impl ContainerSpec {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() || self.name.chars().count() > 100 {
            bail!("Container name must be 1 to 100 characters");
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
            mount_target(&mount.target)?;
            if !targets.insert(&mount.target) {
                bail!("Volume targets must be unique");
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
/// A path a volume can be mounted at in a desktop: absolute and normalized, and none of the
/// system folders or the paths the manager mounts itself.
pub fn mount_target(target: &str) -> Result<()> {
    if !target.starts_with('/')
        || target == "/"
        || target.contains([',', '\n', '\r', '\0'])
        || target[1..]
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        bail!("Invalid volume target");
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
    .any(|p| target == *p || target.starts_with(&format!("{p}/")))
    {
        bail!("Volume target conflicts with desktop infrastructure");
    }
    if target == "/opt" || target == "/var" || target == "/home" {
        bail!("Volume target conflicts with desktop infrastructure");
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
pub struct Container {
    pub id: String,
    pub container: String,
    pub private_network: String,
    pub egress_network: String,
    pub source_image_id: String,
    pub created_at: u64,
    pub spec: ContainerSpec,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub desktop: Option<crate::catalog::DesktopInfo>,
    /// Set once a settings change recreated the container: the container runs this commit of
    /// its earlier files, while `source_image_id` stays the image the desktop started from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_image: Option<WorkingImage>,
    /// Set when the image had a compose: its setup's progress and the values it was given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<crate::compose::Setup>,
}
impl Container {
    /// The image the container is created from.
    pub fn container_image(&self) -> &str {
        self.working_image
            .as_ref()
            .map_or(&self.source_image_id, |image| &image.image_id)
    }
    /// The volumes its compose's volume parameters were given, which its settings neither show
    /// nor change.
    pub fn compose_mounts(&self) -> &[Mount] {
        self.setup.as_ref().map_or(&[], |setup| &setup.mounts)
    }
    /// Every volume the container mounts: those of its settings, then its compose's.
    pub fn mounts(&self) -> Vec<Mount> {
        let mut mounts = self.spec.mounts.clone();
        mounts.extend_from_slice(self.compose_mounts());
        mounts
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
    /// The container a snapshot was taken from. Records saved before environments were called
    /// containers name it `source_environment`.
    #[serde(alias = "source_environment")]
    pub source_container: Option<String>,
    pub kind: String,
    pub created_at: u64,
    pub description: String,
    #[serde(default)]
    pub desktop: Option<crate::catalog::DesktopInfo>,
    /// What a script build ran, so the image can be exported and built again. Snapshots,
    /// imports and builds made before it was recorded have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<BuildConfig>,
    /// What a new container from this image asks for and runs once its desktop is ready. Records
    /// saved before templates were called composes name it `template`.
    #[serde(default, alias = "template", skip_serializing_if = "Option::is_none")]
    pub compose: Option<crate::compose::Compose>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BuildConfig {
    /// The base as the request named it: a catalog reference or a local image ID.
    pub base_image: String,
    pub internet: bool,
    pub script: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Application {
    pub id: String,
    #[serde(alias = "environment_id")]
    pub container_id: String,
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
    #[serde(alias = "environment_id")]
    pub container_id: String,
    pub root: bool,
    pub cwd: String,
    pub created_at: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recording {
    pub id: String,
    #[serde(alias = "environment_id")]
    pub container_id: String,
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
    /// The named stages of a job that reports them, such as a script build, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<crate::stages::Stage>,
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
