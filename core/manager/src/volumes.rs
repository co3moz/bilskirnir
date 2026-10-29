//! Named volumes beyond listing and creating them: their files, read and changed from the
//! dashboard without a container that mounts them, the manager's own volumes, which no helper
//! or desktop gets and which a listing marks, the volumes created for one container, and
//! deleting a volume that nothing mounts.
//!
//! `VOLUME_FILTER` names the label a volume needs for a request to reach it. Every volume the
//! manager creates gets that label; the others stay out of the listing, and a request that names
//! one is answered as if it did not exist. A desktop keeps the volumes it already mounts, so
//! only a new choice is held to the filter.
//!
//! A volume is reached through a short-lived helper container started from the manager's own
//! image, which carries python3 and tar. The helper mounts nothing but that volume, has no
//! network, a read-only root, no capabilities beyond the few an operation needs, and is removed
//! when the operation ends; it never gets the Docker socket. Files are read and written as the
//! desktop user (UID/GID 1001), as a desktop that mounts the volume would.
mod archive;
mod files;
mod owned;
mod remove;

use crate::{
    App,
    error::{Error, Result, missing},
    model::{LABEL, Mount, volume_name},
    process,
};
use anyhow::Context;
use axum::http::StatusCode;
use serde_json::Value;
use std::time::Duration;

pub(crate) use archive::{Extract, Import, VolumeWriter, copy, export, extract, import};
pub(crate) use files::{
    DownloadQuery, delete, download, list, make_directory, move_, read, upload, write,
};
pub(crate) use owned::{OWNER, PARAMETER, RemovedVolumes, mark_owners, removable};
pub(crate) use remove::{mounted_by, remove};

/// Where a helper mounts the volume.
pub(crate) const MOUNT: &str = "/volume";
const DESKTOP_USER: &str = "1001:1001";
/// Hands an empty, root-owned volume to the desktop user, then runs the operation as that user.
const CLAIM: &str = include_str!("../helpers/volume_claim.py");
/// A helper that outlives this is stopped; reading or listing a volume never takes long.
const HELPER_LIMIT: Duration = Duration::from_secs(120);

/// How a helper opens the volume.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Access {
    /// The desktop user, with the volume mounted read-only.
    Read,
    /// The desktop user, writing. Docker creates a volume's root owned by root; while it is still
    /// empty, it is first given to the desktop user, as mounting it over a desktop folder would.
    Write,
    /// Root on a read-only mount, allowed to read every file so an archive is complete.
    Archive,
}

/// A volume a request may open, and the image its helper runs.
pub(crate) struct Volume {
    pub name: String,
    pub image: String,
}

impl App {
    /// Create a volume the manager owns. The caller holds the volume's lock.
    pub(crate) async fn new_volume(&self, name: &str) -> Result<()> {
        if self.optional_inspect("volume", name).await?.is_some() {
            return Err(Error(
                StatusCode::CONFLICT,
                "A volume with this name already exists".into(),
            ));
        }
        self.make_volume(name, &[]).await?;
        Ok(())
    }

    /// Ask Docker for the volume `name` with the manager's label, the one `VOLUME_FILTER` asks
    /// for and `extra`, and return how Docker describes it. Every volume the manager creates is
    /// created here, so none misses the filter's label.
    pub(crate) async fn make_volume(&self, name: &str, extra: &[String]) -> anyhow::Result<Value> {
        let args = create_args(name, self.config.volume_filter.as_deref(), extra);
        self.docker_args(&args).await?;
        let created = self.inspect("volume", name).await?;
        if created["Labels"][LABEL] != "true" || !self.lists_volume(&created) {
            anyhow::bail!("Volume was created by another operation; ownership does not match");
        }
        Ok(created)
    }

    /// The manager container, whose image every helper runs.
    async fn manager_container(&self) -> Result<Value> {
        let manager = self.config.manager.as_ref().ok_or_else(|| {
            Error(
                StatusCode::CONFLICT,
                "Set MANAGER_CONTAINER; volume files are reached through a container started from the manager's image".into(),
            )
        })?;
        let info = self.inspect("container", manager).await?;
        if info["Config"]["Labels"]["thoric.manager"] != "true" {
            return Err(anyhow::anyhow!("MANAGER_CONTAINER does not point to the manager").into());
        }
        Ok(info)
    }

    /// The manager container, to tell its own volumes apart in a listing. A listing goes on
    /// without one: when MANAGER_CONTAINER is unset, names no container or not the manager,
    /// no volume is the manager's.
    pub(crate) async fn listing_manager(&self) -> Result<Option<Value>> {
        let Some(manager) = &self.config.manager else {
            return Ok(None);
        };
        Ok(self
            .optional_inspect("container", manager)
            .await?
            .filter(|info| info["Config"]["Labels"]["thoric.manager"] == "true"))
    }

    /// A volume by that name for a helper to mount, the manager's own refused. Whether it exists
    /// is up to the caller.
    pub(crate) async fn volume_named(&self, name: &str) -> Result<Volume> {
        volume_name(name)?;
        let manager = self.manager_container().await?;
        if manager_volume(&manager, name) {
            return Err(Error(
                StatusCode::FORBIDDEN,
                "This is the manager's own volume; its files cannot be opened here".into(),
            ));
        }
        let image = manager["Image"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing manager image"))?
            .to_owned();
        Ok(Volume {
            name: name.into(),
            image,
        })
    }

    /// Check a volume before any helper mounts it: `docker run` would create a missing one, the
    /// manager's own data volume holds its database and sessions, and one `VOLUME_FILTER` leaves
    /// out is answered as missing.
    pub(crate) async fn open_volume(&self, name: &str) -> Result<Volume> {
        let volume = self.volume_named(name).await?;
        self.optional_inspect("volume", name)
            .await?
            .filter(|info| self.lists_volume(info))
            .ok_or_else(missing)?;
        Ok(volume)
    }

    /// Wait until Docker has the helper container `name`, or `child`, its CLI, has ended. A
    /// reader's volume lock is held until then and no longer.
    pub(crate) async fn helper_started(&self, name: &str, child: &mut tokio::process::Child) {
        for _ in 0..50 {
            if matches!(child.try_wait(), Ok(Some(_)))
                || matches!(self.optional_inspect("container", name).await, Ok(Some(_)))
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Run a short read of the volume under its lock: the whole of a listing or a file read is
    /// quick, and a delete cannot land between the check and the helper's run.
    pub(crate) async fn volume_read<T>(
        &self,
        name: &str,
        work: impl AsyncFnOnce(&Volume) -> Result<T>,
    ) -> Result<T> {
        let _guard = self.hold(&format!("volume:{name}")).await?;
        let volume = self.open_volume(name).await?;
        work(&volume).await
    }

    /// Whether `VOLUME_FILTER` lets a request see and choose the volume `docker volume inspect`
    /// described.
    pub(crate) fn lists_volume(&self, volume: &Value) -> bool {
        listed(self.config.volume_filter.as_deref(), volume)
    }

    /// Look up a volume a request chose for a desktop. One `VOLUME_FILTER` leaves out fails as a
    /// missing one does, so naming it does not reach it.
    pub(crate) async fn chosen_volume(&self, name: &str) -> anyhow::Result<Value> {
        let volume = self.inspect("volume", name).await?;
        if !self.lists_volume(&volume) {
            anyhow::bail!("No such volume: {name}");
        }
        Ok(volume)
    }

    /// Look up the volumes a request chose for a desktop: those of its settings, and those its
    /// compose was given, each named with the folder it was chosen for.
    pub(crate) async fn chosen_volumes(
        &self,
        mounts: &[Mount],
        compose: &[&Mount],
    ) -> anyhow::Result<()> {
        for mount in mounts {
            self.chosen_volume(&mount.volume)
                .await
                .context("Volume not found")?;
        }
        for mount in compose {
            self.chosen_volume(&mount.volume).await.with_context(|| {
                format!(
                    "Volume not found: {}, chosen for {}. Create it first, or choose another volume",
                    mount.volume, mount.target
                )
            })?;
        }
        Ok(())
    }

    /// Hold the locks of the volumes `names` until the guards are dropped, each once and in name
    /// order. A job that reads the container records to decide about a volume, or saves a record
    /// that mounts one, takes them this way after its container's lock, so two such jobs never
    /// wait on each other: a creation from its check of the volumes to its record, and a delete
    /// from its check of the records to the removal. It waits as a job waits for its resource,
    /// while a request that waits for a volume gives up after [`crate::jobs::HOLD_WAIT`].
    pub(crate) async fn lock_volumes<'a>(
        &self,
        names: impl IntoIterator<Item = &'a str>,
    ) -> Vec<tokio::sync::OwnedMutexGuard<()>> {
        let names = names.into_iter().collect::<std::collections::BTreeSet<_>>();
        let mut guards = Vec::with_capacity(names.len());
        for name in names {
            let lock = self.resource_lock(&format!("volume:{name}"));
            guards.push(lock.lock_owned().await);
        }
        guards
    }

    /// Run `command` in a helper and return what it printed. A helper's own message on stderr
    /// becomes the error; `CONFLICT:` marks an existing destination.
    pub(crate) async fn run_helper(
        &self,
        volume: &Volume,
        access: Access,
        command: &[String],
        input: Option<Vec<u8>>,
        limit: usize,
    ) -> Result<Vec<u8>> {
        let name = helper_name();
        let args = helper_args(volume, access, &name, input.is_some(), command);
        let output = process::capture_input(
            self.command(&args),
            HELPER_LIMIT,
            limit,
            process::Overflow::Reject,
            input,
        )
        .await;
        let output = match output {
            Ok(output) => output,
            Err(error) => {
                // Stopping the CLI leaves its container running; remove it by name.
                let _ = self.docker(&["rm", "--force", &name]).await;
                return Err(error.into());
            }
        };
        if !output.status.success() {
            return Err(helper_error(&output.stderr));
        }
        Ok(output.stdout)
    }
}

/// Whether the manager container mounts this volume, such as its data volume.
pub(crate) fn manager_volume(manager: &Value, name: &str) -> bool {
    manager["Mounts"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|mount| mount["Type"] == "volume" && mount["Name"] == name)
}

/// The `docker volume ls` whose lines a listing shows: the volumes `filter` lets a request reach.
pub(crate) fn list_args(filter: Option<&str>) -> Vec<String> {
    let mut args = vec!["volume".to_owned(), "ls".into()];
    if let Some(filter) = filter {
        args.extend(["--filter".into(), format!("label={filter}")]);
    }
    args.extend(["--format".into(), "{{json .}}".into()]);
    args
}

/// Whether a volume carries the label `filter` asks for, read as Docker's `label` filter reads
/// it: `key=value` needs that value, a bare `key` the label with any value. No filter lists all.
pub(crate) fn listed(filter: Option<&str>, volume: &Value) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    let (key, value) = match filter.split_once('=') {
        Some((key, value)) => (key, Some(value)),
        None => (filter, None),
    };
    match (&volume["Labels"][key], value) {
        (Value::String(found), Some(value)) => found == value,
        (Value::String(_), None) => true,
        _ => false,
    }
}

/// The labels of a volume the manager creates: its ownership label, and the one `filter` asks
/// for, so that every volume made here is one a request can see and choose.
pub(crate) fn labels(filter: Option<&str>) -> Vec<String> {
    let mut labels = vec![format!("{LABEL}=true")];
    if let Some(filter) = filter
        && filter.split_once('=').map_or(filter, |(key, _)| key) != LABEL
    {
        labels.push(filter.into());
    }
    labels
}

/// The `docker volume create` arguments of the volume `name`, with the labels `filter` asks for and
/// `extra`.
fn create_args(name: &str, filter: Option<&str>, extra: &[String]) -> Vec<String> {
    let mut args = vec!["volume".to_owned(), "create".into()];
    for label in labels(filter).into_iter().chain(extra.iter().cloned()) {
        args.extend(["--label".into(), label]);
    }
    args.push(name.into());
    args
}

/// Add `manager` to each line of `docker volume ls`: whether the manager mounts that volume, so a
/// client offers it for no mount and opens none of its files. Without a manager, none is marked.
pub(crate) fn mark_manager_volumes(volumes: &mut [Value], manager: Option<&Value>) {
    for volume in volumes {
        let own = match (manager, volume["Name"].as_str()) {
            (Some(manager), Some(name)) => manager_volume(manager, name),
            _ => false,
        };
        if let Value::Object(fields) = volume {
            fields.insert("manager".into(), own.into());
        }
    }
}

/// Refuse a desktop the manager's own volumes: its data volume holds the database and sessions.
pub(crate) fn refuse_manager_volumes(manager: &Value, mounts: &[Mount]) -> anyhow::Result<()> {
    if let Some(mount) = mounts.iter().find(|m| manager_volume(manager, &m.volume)) {
        anyhow::bail!(
            "{} is the manager's own volume and cannot be mounted in a desktop",
            mount.volume
        );
    }
    Ok(())
}

pub(crate) fn helper_name() -> String {
    format!("thoric-volume-{}", uuid::Uuid::new_v4().simple())
}

/// The `docker run` arguments of a helper that runs `command` against the volume.
pub(crate) fn helper_args(
    volume: &Volume,
    access: Access,
    name: &str,
    interactive: bool,
    command: &[String],
) -> Vec<String> {
    let mut args: Vec<String> = [
        "run",
        "--rm",
        "--name",
        name,
        "--label",
        "thoric.volume-helper=true",
        // The helper streams whole archives through its stdout; the json-file driver would copy them
        // into its container log until --rm removes it.
        "--log-driver",
        "none",
        "--network",
        "none",
        "--read-only",
        "--tmpfs",
        "/tmp",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges",
        "--pids-limit",
        "64",
    ]
    .map(str::to_owned)
    .to_vec();
    if interactive {
        args.push("--interactive".into());
    }
    let mut mount = format!("type=volume,src={},dst={MOUNT}", volume.name);
    let (program, rest) = command.split_first().expect("a helper command");
    match access {
        Access::Read => {
            mount.push_str(",readonly");
            args.extend(["--user".into(), DESKTOP_USER.into()]);
            args.extend([
                "--mount".into(),
                mount,
                "--entrypoint".into(),
                program.clone(),
            ]);
            args.push(volume.image.clone());
            args.extend(rest.iter().cloned());
        }
        Access::Write => {
            // Root only long enough to claim an empty volume; it switches to the desktop user
            // before it touches a file, and no-new-privileges keeps it there.
            args.extend(["--user".into(), "0:0".into()]);
            for capability in ["CHOWN", "SETUID", "SETGID"] {
                args.extend(["--cap-add".into(), capability.into()]);
            }
            args.extend([
                "--mount".into(),
                mount,
                "--entrypoint".into(),
                "python3".into(),
            ]);
            args.push(volume.image.clone());
            args.extend(["-c".into(), CLAIM.into()]);
            args.extend(command.iter().cloned());
        }
        Access::Archive => {
            mount.push_str(",readonly");
            args.extend(["--user".into(), "0:0".into()]);
            args.extend(["--cap-add".into(), "DAC_READ_SEARCH".into()]);
            args.extend([
                "--mount".into(),
                mount,
                "--entrypoint".into(),
                program.clone(),
            ]);
            args.push(volume.image.clone());
            args.extend(rest.iter().cloned());
        }
    }
    args
}

/// A helper's failure as the dashboard shows it: its own message, without Python's prefix noise.
fn helper_error(stderr: &[u8]) -> Error {
    // Helpers print one line; Docker's own failure leads with its reason and ends with advice.
    let text = String::from_utf8_lossy(stderr);
    let message = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .to_owned();
    if let Some(conflict) = message.strip_prefix("CONFLICT: ") {
        return Error(StatusCode::CONFLICT, conflict.into());
    }
    if message.is_empty() {
        return Error(
            StatusCode::BAD_GATEWAY,
            "The volume helper ended without saying why".into(),
        );
    }
    // Docker that cannot be reached, its daemon's failure or a helper that crashed.
    if let Some(status) = crate::error::answer_status(&text) {
        return Error(status, message);
    }
    if message.contains("[Errno 13]") {
        return Error(
            StatusCode::FORBIDDEN,
            format!("{message}. The desktop user (UID 1001) needs permission here."),
        );
    }
    Error(StatusCode::BAD_REQUEST, message)
}

#[cfg(test)]
#[path = "volumes_tests.rs"]
mod tests;
