//! Changing a container's settings after it was created.
//!
//! Docker fixes a container's environment variables, devices, shared memory size and mounts
//! when it creates the container, and `docker update` changes a resource limit but cannot
//! remove one. The name and internet access therefore change in place and limits through
//! `docker update`. Every other change recreates the container from a commit of its files:
//! the desktop restarts, but nothing that was in its container layer is lost.
use crate::{App, model::*};
use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};

/// What refuses a job while the container an interrupted settings change set aside may hold the
/// only copy of the files, which a record from before the commit was saved first cannot rule out.
pub(crate) const PREVIOUS_LEFT: &str = "is left from an interrupted settings change; rename it back to the container's name, or check its files and remove it, before changing settings or retrying";

/// Settings an existing container accepts. The image is not one of them: a reset recreates
/// the desktop from its image, and a snapshot can start a new container.
const EDITABLE: &[&str] = &[
    "name",
    "resolution",
    "gpu",
    "internet",
    "audio",
    "microphone",
    "sudo",
    "gamepad",
    "hostname",
    "mac_address",
    "machine_id",
    "ports",
    "cpus",
    "memory_mb",
    "shm_mb",
    "mounts",
];

/// The settings a request changes, split by how they apply.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Changes {
    /// Applied to the existing container.
    pub live: Vec<&'static str>,
    /// Applied by recreating the container, which restarts the desktop.
    pub restart: Vec<&'static str>,
}

/// Apply a partial settings object to the current settings.
pub(crate) fn merged_spec(
    current: &ContainerSpec,
    request: &Map<String, Value>,
) -> Result<ContainerSpec> {
    let mut value = serde_json::to_value(current)?;
    for (key, field) in request {
        if key == "image" {
            if field.as_str() != Some(current.image.as_str()) {
                bail!(
                    "The image of an existing container cannot change; a reset recreates the container from its image, and a snapshot can start a new container"
                );
            }
        } else if EDITABLE.contains(&key.as_str()) {
            value[key] = field.clone();
        } else {
            bail!("Unknown container setting: {key}");
        }
    }
    let spec: ContainerSpec =
        serde_json::from_value(value).context("Invalid container settings")?;
    spec.validate()?;
    // Another value takes over another machine's identity; none would give the desktop a new
    // one at its next recreation without saying so.
    for (key, old, new) in [
        ("hostname", &current.hostname, &spec.hostname),
        ("MAC address", &current.mac_address, &spec.mac_address),
        ("machine ID", &current.machine_id, &spec.machine_id),
    ] {
        if old.is_some() && new.is_none() {
            bail!("A container's {key} can be changed but not removed");
        }
    }
    Ok(spec)
}

/// Which settings differ, and whether each applies in place or recreates the container.
pub(crate) fn classify(old: &ContainerSpec, new: &ContainerSpec) -> Changes {
    let mut changes = Changes::default();
    let mut note = |changed: bool, live: bool, name: &'static str| match (changed, live) {
        (false, _) => {}
        (true, true) => changes.live.push(name),
        (true, false) => changes.restart.push(name),
    };
    note(old.name != new.name, true, "name");
    note(old.internet != new.internet, true, "internet");
    // The manager proxies the ports itself; Docker publishes none of them.
    note(old.ports != new.ports, true, "ports");
    // `docker update` changes or adds a limit, but silently ignores a request to remove one.
    note(old.cpus != new.cpus, new.cpus.is_some(), "cpus");
    note(
        old.memory_mb != new.memory_mb,
        new.memory_mb.is_some(),
        "memory_mb",
    );
    // Environment variables, devices, shared memory and mounts are fixed at creation.
    note(old.resolution != new.resolution, false, "resolution");
    note(old.gpu != new.gpu, false, "gpu");
    note(old.audio != new.audio, false, "audio");
    note(old.microphone != new.microphone, false, "microphone");
    note(old.sudo != new.sudo, false, "sudo");
    note(old.gamepad != new.gamepad, false, "gamepad");
    // Docker fixes the hostname and MAC address at creation, and software reads the machine ID
    // when it starts.
    note(old.hostname != new.hostname, false, "hostname");
    note(old.mac_address != new.mac_address, false, "mac_address");
    note(old.machine_id != new.machine_id, false, "machine_id");
    note(old.shm_mb != new.shm_mb, false, "shm_mb");
    let by_target = |mounts: &[Mount]| {
        let mut mounts = mounts.to_vec();
        mounts.sort_by(|a, b| a.target.cmp(&b.target));
        mounts
    };
    note(
        by_target(&old.mounts) != by_target(&new.mounts),
        false,
        "mounts",
    );
    changes
}

/// `docker update` arguments for limits that change in place, without the container. Docker
/// keeps the old swap limit unless told otherwise, and it must not fall below the memory limit;
/// twice the memory is the default that `docker create` gave it.
pub(crate) fn update_args(old: &ContainerSpec, new: &ContainerSpec) -> Option<Vec<String>> {
    let mut args = vec!["update".to_owned()];
    if old.cpus != new.cpus
        && let Some(cpus) = new.cpus
    {
        args.extend(["--cpus".into(), cpus.to_string()]);
    }
    if old.memory_mb != new.memory_mb
        && let Some(memory) = new.memory_mb
    {
        args.extend([
            "--memory".into(),
            format!("{memory}m"),
            "--memory-swap".into(),
            format!("{}m", memory.saturating_mul(2)),
        ]);
    }
    (args.len() > 1).then_some(args)
}

impl App {
    /// Check a settings request and report what it would change, without changing anything.
    pub(crate) fn settings_preview(
        &self,
        env: &Container,
        request: &Map<String, Value>,
    ) -> Result<(ContainerSpec, Changes)> {
        let spec = merged_spec(&env.spec, request)?;
        crate::compose::check_mounts(&spec.mounts, env.compose_mounts())?;
        // Only a feature being switched on needs the installation's permission; a desktop that
        // already has one keeps it after the installation turns it off, as it did before.
        let mut enabled = spec.clone();
        enabled.audio &= !env.spec.audio;
        enabled.microphone &= !env.spec.microphone;
        enabled.gpu &= !env.spec.gpu;
        enabled.internet &= !env.spec.internet;
        enabled.sudo &= !env.spec.sudo;
        enabled.gamepad &= !env.spec.gamepad;
        self.installation_allows(&enabled)?;
        let changes = classify(&env.spec, &spec);
        // A change that creates the container again gives it the host's GPU again, as a reset
        // does, and a GPU desktop without one stops at its renderer check.
        if spec.gpu
            && !changes.restart.is_empty()
            && let Some(refusal) = crate::gpu::refusal(&self.config)
        {
            bail!(refusal);
        }
        Ok((spec, changes))
    }

    /// Apply a settings request to a container.
    pub async fn update_container(
        &self,
        id: &str,
        request: Map<String, Value>,
        job_id: &str,
    ) -> Result<Value> {
        let mut env = self.container(id).map_err(|e| anyhow::anyhow!(e.1))?;
        let (spec, changes) = self.settings_preview(&env, &request)?;
        let result = json!({"container_id": id, "live": changes.live, "restart": changes.restart});
        if changes.live.is_empty() && changes.restart.is_empty() {
            self.append_log(job_id, "The settings are unchanged.")?;
            return Ok(result);
        }
        let mounts = spec.mounts.iter().chain(env.compose_mounts());
        let mounts = mounts.cloned().collect::<Vec<_>>();
        // A new container gets the mounts, so check them before the desktop stops.
        if !changes.restart.is_empty() && !mounts.is_empty() {
            let manager = self
                .config
                .manager
                .as_deref()
                .context("MANAGER_CONTAINER is required")?;
            let manager_info = self.inspect("container", manager).await?;
            crate::volumes::refuse_manager_volumes(&manager_info, &mounts)?;
        }
        // A new container would get an empty volume in place of a missing one. A volume the
        // container already mounts stays, while a new choice has to be one VOLUME_FILTER lists.
        let mounted = env.mounts();
        for mount in &mounts {
            let found = if mounted.iter().any(|m| m.volume == mount.volume) {
                self.inspect("volume", &mount.volume).await
            } else {
                self.chosen_volume(&mount.volume).await
            };
            found.with_context(|| format!("Volume not found: {}", mount.volume))?;
        }
        if self
            .optional_inspect("container", &env.container)
            .await?
            .is_none()
        {
            env.spec = spec;
            self.db.put(CONTAINERS, id, &env)?;
            self.append_log(
                job_id,
                "Saved the settings. The container is missing, so they apply when setup is retried.",
            )?;
            return Ok(result);
        }
        let info = self.owned(&env).await?;
        let container_id = info["Id"]
            .as_str()
            .context("Missing container ID")?
            .to_owned();
        if !changes.restart.is_empty() {
            self.recreate(env, spec, &info, job_id).await?;
            return Ok(result);
        }
        if let Some(mut args) = update_args(&env.spec, &spec) {
            args.push(container_id);
            self.docker_args(&args).await?;
            self.append_log(job_id, "Changed the resource limits in place.")?;
        }
        if env.spec.internet != spec.internet {
            self.set_internet(&mut env, spec.internet).await?;
            self.append_log(
                job_id,
                if spec.internet {
                    "Connected the container to the internet."
                } else {
                    "Disconnected the container from the internet."
                },
            )?;
        }
        env.spec = spec;
        self.db.put(CONTAINERS, id, &env)?;
        self.append_log(job_id, "Saved the settings. The container kept running.")?;
        Ok(result)
    }

    /// Replace the container with one created from a commit of its files and the new settings.
    /// The old container is renamed aside, not removed, until its replacement has started.
    async fn recreate(
        &self,
        mut env: Container,
        spec: ContainerSpec,
        info: &Value,
        job_id: &str,
    ) -> Result<()> {
        let old_id = info["Id"]
            .as_str()
            .context("Missing container ID")?
            .to_owned();
        let running = info["State"]["Running"] == true;
        let previous = format!("{}-previous", env.container);
        if self
            .optional_inspect("container", &previous)
            .await?
            .is_some()
        {
            // The container just inspected is the replacement an interrupted change created
            // from its commit, so the one set aside has nothing it lacks. Without the commit on
            // record, the container set aside may hold the only copy of the files.
            if env.working_image.is_none() {
                bail!("{previous} {PREVIOUS_LEFT}");
            }
            self.remove_previous(&env, job_id).await?;
        }
        self.plan_stages(job_id, &crate::stages::RECREATE_STAGES)?;
        if running {
            self.begin_stage(job_id, "stop")?;
            self.append_log(job_id, "Stopping the container. Open applications close.")?;
            self.docker(&["stop", "--time", "30", &old_id]).await?;
        }
        self.begin_stage(job_id, "commit")?;
        self.append_log(
            job_id,
            "Saving the container's files for the new container. Named volumes stay where they are.",
        )?;
        let reference = format!("bilskirnir/container:{job_id}");
        let committed = async {
            self.docker_logged(
                job_id,
                &[
                    "commit".into(),
                    "--change".into(),
                    format!("LABEL {LABEL}=true"),
                    // Snapshots taken later keep this layer, and their history says what it is.
                    "--message".into(),
                    format!("Bilskirnir settings change of {}", env.spec.name),
                    old_id.clone(),
                    reference.clone(),
                ],
            )
            .await?;
            let image = self.inspect("image", &reference).await?;
            let mounts = spec.mounts.iter().chain(env.compose_mounts()).cloned();
            crate::docker::image_volumes_mounted(&image, &mounts.collect::<Vec<_>>())?;
            self.docker(&["rename", &old_id, &previous]).await?;
            anyhow::Ok(image["Id"].as_str().context("Missing image ID")?.to_owned())
        }
        .await;
        let image_id = match committed {
            Ok(image_id) => image_id,
            Err(error) => {
                if running {
                    self.docker(&["start", &old_id]).await?;
                }
                let _ = self.docker(&["image", "rm", &reference]).await;
                return Err(error);
            }
        };
        let before = env.clone();
        let earlier = env.working_image.replace(WorkingImage {
            image_id,
            reference: reference.clone(),
        });
        env.spec = spec;
        crate::identity::adopt(&mut env, Some(info));
        // The files are in the commit and in the container set aside from here on, so the record
        // says so before anything is created: a retry after an interruption then creates the
        // container from the commit instead of an empty one from the source image.
        self.db.put(CONTAINERS, &env.id, &env)?;
        self.begin_stage(job_id, "create")?;
        let replaced = async {
            self.docker_args(&self.container_args(&env))
                .await
                .map_err(|error| self.gpu_error(&env, error))?;
            self.write_machine_id(&env).await?;
            self.hand_over_mount_parents(&env).await?;
            if env.spec.internet {
                self.connect_network(&env, &env.egress_network, &env.container)
                    .await?;
            }
            if running {
                self.docker(&["start", &env.container])
                    .await
                    .map_err(|error| self.gpu_error(&env, error))?;
            }
            anyhow::Ok(())
        }
        .await;
        if let Err(error) = replaced {
            self.append_log(
                job_id,
                "The new container could not be started; restoring the previous one.",
            )?;
            // Remove only the container this attempt created, never the original.
            if let Some(created) = self.optional_inspect("container", &env.container).await?
                && created["Id"] != old_id.as_str()
                && created["Config"]["Labels"]["bilskirnir.environment"] == env.id.as_str()
                && let Some(created_id) = created["Id"].as_str()
            {
                self.docker(&["rm", "--force", created_id]).await?;
            }
            self.docker(&["rename", &old_id, &env.container]).await?;
            if running {
                self.docker(&["start", &old_id]).await?;
            }
            // Only once the previous container is back under its name; until then the record
            // keeps naming the commit, which a retry can still create a container from.
            self.db.put(CONTAINERS, &before.id, &before)?;
            let _ = self.docker(&["image", "rm", &reference]).await;
            return Err(error);
        }
        env.last_error = None;
        self.db.put(CONTAINERS, &env.id, &env)?;
        self.remove_container_records(&env.id)?;
        self.begin_stage(job_id, "cleanup")?;
        self.docker(&["rm", "--force", &old_id])
            .await
            .with_context(|| {
                format!("The container uses the new settings, but {previous} could not be removed")
            })?;
        if let Some(image) = earlier {
            self.release_working_image(&image, job_id).await?;
        }
        self.append_log(
            job_id,
            if running {
                "Recreated and started the container with the new settings. Its files were kept."
            } else {
                "Recreated the stopped container with the new settings. Its files were kept; start it to use them."
            },
        )?;
        Ok(())
    }

    /// Remove the container an interrupted settings change set aside, once its replacement
    /// stands or the container's files go anyway. Nothing but that container is touched: it
    /// must carry this container's labels, and only the commit it was created from loses its
    /// tag, unless the record still names that commit.
    pub(crate) async fn remove_previous(&self, env: &Container, job_id: &str) -> Result<()> {
        let previous = format!("{}-previous", env.container);
        let Some(info) = self.optional_inspect("container", &previous).await? else {
            return Ok(());
        };
        if info["Config"]["Labels"][LABEL] != "true"
            || info["Config"]["Labels"]["bilskirnir.environment"] != env.id.as_str()
        {
            bail!("{previous} was not created for this container; check and remove it by hand");
        }
        let id = info["Id"].as_str().context("Missing container ID")?;
        self.docker(&["rm", "--force", id]).await?;
        self.append_log(
            job_id,
            &format!("Removed {previous}, which an interrupted settings change had set aside."),
        )?;
        if let Some(image) = info["Config"]["Image"].as_str()
            && image.starts_with("bilskirnir/container:")
            && env
                .working_image
                .as_ref()
                .is_none_or(|working| working.reference != image)
        {
            let _ = self.docker(&["image", "rm", image]).await;
        }
        Ok(())
    }

    /// Remove the tag of a working image no container runs anymore. When a snapshot builds on
    /// it, Docker keeps its layers and only removes the tag.
    pub(crate) async fn release_working_image(
        &self,
        image: &WorkingImage,
        job_id: &str,
    ) -> Result<()> {
        // Only a tag this container created, and only while it still names the same image.
        let Ok(info) = self.inspect("image", &image.reference).await else {
            return Ok(());
        };
        if info["Id"] != image.image_id.as_str() {
            return Ok(());
        }
        match self.docker(&["image", "rm", &image.reference]).await {
            Ok(_) => self.append_log(
                job_id,
                "Removed the image that carried the container's files across settings changes.",
            ),
            Err(error) => self.append_log(job_id, &format!("Kept {}: {error:#}", image.reference)),
        }
    }
}

#[cfg(test)]
#[path = "reconfigure_tests.rs"]
mod tests;
