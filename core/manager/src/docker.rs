use crate::{App, model::*, volumes::RemovedVolumes};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};

/// How long a logged Docker command (a build, a pull, a script or a setup step) may run, unless
/// its caller gives a longer limit.
pub(crate) const LOGGED_LIMIT: Duration = Duration::from_secs(3600);

impl App {
    pub fn command(&self, args: &[String]) -> Command {
        let mut cmd = Command::new(&self.config.docker_bin);
        cmd.args(args).kill_on_drop(true);
        cmd
    }
    pub async fn docker(&self, args: &[&str]) -> Result<Vec<u8>> {
        let args = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        self.docker_args(&args).await
    }
    pub async fn docker_args(&self, args: &[String]) -> Result<Vec<u8>> {
        let out = crate::process::capture(
            self.command(args),
            Duration::from_secs(120),
            4 * 1024 * 1024,
            crate::process::Overflow::Reject,
        )
        .await?;
        if !out.status.success() {
            bail!("Docker: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(out.stdout)
    }
    pub async fn inspect(&self, kind: &str, name: &str) -> Result<Value> {
        let data = self.docker(&[kind, "inspect", name]).await?;
        let mut values: Vec<Value> = serde_json::from_slice(&data)?;
        values.pop().context("Docker object not found")
    }
    pub async fn owned(&self, env: &Container) -> Result<Value> {
        let info = self.inspect("container", &env.container).await?;
        if info["Config"]["Labels"][LABEL] != "true"
            || info["Config"]["Labels"]["thoric.environment"] != env.id
        {
            bail!("Container ownership does not match");
        }
        Ok(info)
    }
    pub async fn optional_inspect(&self, kind: &str, name: &str) -> Result<Option<Value>> {
        match self.inspect(kind, name).await {
            Ok(info) => Ok(Some(info)),
            Err(error) => {
                // Listing must succeed before an inspect failure can mean 'absent'.
                let args = if kind == "container" {
                    vec!["ps", "-a", "--format", "{{.Names}}"]
                } else {
                    vec![kind, "ls", "--format", "{{.Name}}"]
                };
                let names = self.docker(&args).await?;
                if String::from_utf8_lossy(&names).lines().any(|n| n == name) {
                    return Err(error);
                }
                Ok(None)
            }
        }
    }
    pub fn check_network(env: &Container, info: &Value, internal: bool) -> Result<()> {
        if info["Labels"][LABEL] != "true"
            || info["Labels"]["thoric.environment"] != env.id
            || info["Internal"] != internal
            || info["Driver"] != "bridge"
        {
            bail!("Network ownership or type does not match");
        }
        Ok(())
    }
    async fn ensure_network(&self, env: &Container, name: &str, internal: bool) -> Result<Value> {
        if let Some(info) = self.optional_inspect("network", name).await? {
            Self::check_network(env, &info, internal)?;
            return Ok(info);
        }
        let mut args = vec![
            "network",
            "create",
            "--label",
            "thoric.managed=true",
            "--label",
        ];
        let owner = format!("thoric.environment={}", env.id);
        args.push(&owner);
        if internal {
            args.push("--internal");
        }
        args.push(name);
        let id = self.docker(&args).await?;
        self.inspect("network", String::from_utf8_lossy(&id).trim())
            .await
    }
    pub fn container(&self, id: &str) -> crate::error::Result<Container> {
        self.db
            .get(CONTAINERS, id)
            .map_err(crate::error::Error::from)
    }
    /// Settings the installation has switched off cannot be chosen for a desktop. A desktop that
    /// has one keeps it in its settings, also when it is created again; the manager only refuses
    /// to carry what it carries itself, the audio channels and gamepad input, while it is off.
    pub(crate) fn installation_allows(&self, spec: &ContainerSpec) -> Result<()> {
        if spec.internet && !self.config.internet {
            bail!(
                "Internet access is disabled for this installation (INTERNET_ENABLED=false); use the container without internet"
            );
        }
        if spec.sudo && !self.config.sudo {
            bail!(
                "sudo is disabled for this installation (SUDO_ENABLED=false); use the container without sudo"
            );
        }
        if spec.gamepad && !self.config.gamepad {
            bail!("The virtual gamepad is disabled for this installation (GAMEPAD_ENABLED=false)");
        }
        if spec.audio && !self.config.audio {
            bail!("Audio is disabled for this installation");
        }
        if spec.microphone && !self.config.microphone {
            bail!("Microphone is disabled for this installation");
        }
        if spec.gpu
            && let Some(refusal) = crate::gpu::refusal(&self.config)
        {
            bail!(refusal);
        }
        Ok(())
    }
    /// A Docker error from creating or starting a GPU desktop, with the setting behind the
    /// device it names.
    pub(crate) fn gpu_error(&self, env: &Container, error: anyhow::Error) -> anyhow::Error {
        if env.spec.gpu {
            crate::gpu::explain(error, &crate::gpu::routes(&self.config).used)
        } else {
            error
        }
    }
    pub async fn create_container(
        &self,
        id: &str,
        spec: ContainerSpec,
        parameters: std::collections::BTreeMap<String, String>,
        job_id: &str,
    ) -> Result<Value> {
        spec.validate()?;
        self.installation_allows(&spec)?;
        let manager = self
            .config
            .manager
            .as_ref()
            .context("Set MANAGER_CONTAINER; the service must run in Docker")?;
        let manager_info = self.inspect("container", manager).await?;
        if manager_info["Config"]["Labels"]["thoric.manager"] != "true" {
            bail!("MANAGER_CONTAINER does not point to the manager");
        }
        // Cheap checks come before a catalog image download that can take minutes.
        crate::volumes::refuse_manager_volumes(&manager_info, &spec.mounts)?;
        for m in &spec.mounts {
            self.chosen_volume(&m.volume)
                .await
                .context("Volume not found")?;
        }
        self.ensure_catalog_image(&spec.image, job_id).await?;
        let image = self
            .inspect("image", &spec.image)
            .await
            .context("Image not available. Build or pull it first")?;
        if image["Config"]["Labels"]["thoric.desktop"] != "true" {
            bail!("Image must be derived from a Thoric desktop image");
        }
        let image_id = image["Id"].as_str().context("Missing image ID")?;
        let compose = self.pin_compose(id, &spec.image, image_id, &parameters)?;
        let compose_mounts = compose
            .as_ref()
            .map(|(_, pinned)| pinned.mounts())
            .unwrap_or_default();
        crate::compose::check_mounts(&spec.mounts, &compose_mounts)?;
        // An image's compose names its default volume; it cannot hand a desktop the manager's
        // database either.
        crate::volumes::refuse_manager_volumes(&manager_info, &compose_mounts)?;
        let own_volumes = compose
            .as_ref()
            .map(|(_, pinned)| pinned.new_volumes(id))
            .unwrap_or_default();
        for mount in compose_mounts
            .iter()
            .filter(|mount| !own_volumes.iter().any(|(_, own)| *own == mount.volume))
        {
            self.chosen_volume(&mount.volume)
                .await
                .with_context(|| {
                    format!(
                        "Volume not found: {}, chosen for {}. Create it first, or choose another volume",
                        mount.volume, mount.target
                    )
                })?;
        }
        let all_mounts = spec
            .mounts
            .iter()
            .chain(&compose_mounts)
            .cloned()
            .collect::<Vec<_>>();
        image_volumes_mounted(&image, &all_mounts)?;
        let container = format!("thoric-{id}");
        let mut spec = spec;
        // A container from a snapshot gets an identity of its own, not the source container's.
        crate::identity::assign(&mut spec, &container);
        let env = Container {
            id: id.into(),
            container,
            private_network: format!("thoric-{id}-private"),
            egress_network: format!("thoric-{id}-egress"),
            source_image_id: image_id.into(),
            spec,
            created_at: now(),
            last_error: None,
            desktop: crate::catalog::from_image(&image),
            working_image: None,
            setup: compose.as_ref().map(|(image, pinned)| pinned.setup(image)),
        };
        // The container's own volumes are made before its record, so no record names one that
        // was never made; a missing one later was removed, and the rule for missing volumes holds.
        // The other way round too: a volume made here goes again when the record cannot be
        // saved, so no volume names an owner that never existed.
        let mut created = Vec::new();
        let recorded = async {
            for (parameter, volume) in &own_volumes {
                let lock = self.resource_lock(&format!("volume:{volume}"));
                let _guard = lock.lock().await;
                if self.container_volume(id, parameter, volume).await? {
                    created.push(volume.to_string());
                    self.append_log(
                        job_id,
                        &format!("Created the volume {volume} for {parameter}."),
                    )?;
                }
            }
            // Save intent first: failed setup and interrupted jobs remain discoverable and removable.
            if let Some((_, pinned)) = &compose {
                self.save_pinned(id, pinned)?;
            }
            self.db.put(CONTAINERS, id, &env)
        }
        .await;
        if let Err(error) = recorded {
            for volume in &created {
                if self.docker(&["volume", "rm", volume]).await.is_ok() {
                    let _ = self.append_log(job_id, &format!("Removed the volume {volume} again."));
                }
            }
            return Err(error);
        }
        let result = self.provision(&env, manager).await;
        if let Err(ref e) = result {
            let mut failed = env.clone();
            failed.last_error = Some(format!("{e:#}"));
            self.db.put(CONTAINERS, id, &failed)?;
            // Keep intent and any partial resources for explicit retry/removal; never touch unrelated objects.
        }
        result?;
        self.run_setup(id, job_id).await?;
        Ok(json!({"container_id":id}))
    }
    pub async fn retry_container(&self, id: &str, job_id: &str) -> Result<Value> {
        let mut env = self.container(id).map_err(|e| anyhow::anyhow!(e.1))?;
        let manager = self
            .config
            .manager
            .as_deref()
            .context("MANAGER_CONTAINER is required")?;
        let info = self.inspect("container", manager).await?;
        if info["Config"]["Labels"]["thoric.manager"] != "true" {
            bail!("Manager label does not match");
        }
        let existing = self.optional_inspect("container", &env.container).await?;
        if existing.is_none() && env.working_image.is_none() {
            // A settings change set the container aside before it recorded the commit, so that
            // container may hold the only copy of the files; never replace it with an empty one.
            let previous = format!("{}-previous", env.container);
            if self
                .optional_inspect("container", &previous)
                .await?
                .is_some()
            {
                bail!("{previous} {}", crate::reconfigure::PREVIOUS_LEFT);
            }
            // A container recreated from the source image lacks what the compose setup changed.
            crate::compose::setup_again(&mut env);
            // One restored from another installation's backup may name that installation's image
            // ID; the catalog image it names is downloaded instead. A desktop with the manager's
            // own volume is refused below, before anything is downloaded, and an image that is
            // still missing is reported there and kept on the record.
            if crate::volumes::refuse_manager_volumes(&info, &env.mounts()).is_ok()
                && let Err(error) = self.source_image(&mut env, job_id).await
            {
                let _ = self.append_log(job_id, &format!("{error:#}"));
            }
        }
        crate::identity::adopt(&mut env, existing.as_ref());
        self.db.put(CONTAINERS, id, &env)?;
        let result = self.provision(&env, manager).await;
        env.last_error = result.as_ref().err().map(|e| format!("{e:#}"));
        self.db.put(CONTAINERS, id, &env)?;
        result?;
        // The container stands, so one a settings change set aside has finished its part. With
        // the commit on record, the files are in the container created from it.
        if env.working_image.is_some() {
            self.remove_previous(&env, job_id).await?;
        }
        // Also a setup that failed or was interrupted earlier.
        self.run_setup(id, job_id).await?;
        Ok(json!({"container_id":id}))
    }
    async fn provision(&self, env: &Container, manager: &str) -> Result<()> {
        self.provision_and(env, manager, true).await
    }
    /// Create a container its record describes, with its networks, and leave it stopped, as a
    /// restore does.
    pub(crate) async fn create_stopped(&self, env: &Container, manager: &str) -> Result<()> {
        self.provision_and(env, manager, false).await
    }
    async fn provision_and(&self, env: &Container, manager: &str, start: bool) -> Result<()> {
        // Validate all existing resources before creating or starting anything.
        env.spec.validate()?;
        // A GPU desktop without a GPU exits at its renderer check and restarts forever.
        if env.spec.gpu
            && let Some(refusal) = crate::gpu::refusal(&self.config)
        {
            bail!(refusal);
        }
        let manager_info = self.inspect("container", manager).await?;
        let mounts = env.mounts();
        crate::volumes::refuse_manager_volumes(&manager_info, &mounts)?;
        // These were chosen when the container was created, so a volume VOLUME_FILTER no longer
        // lists stays mounted. Also a volume created for this container: one that is gone may
        // have held its work.
        for mount in &mounts {
            self.inspect("volume", &mount.volume)
                .await
                .with_context(|| {
                    format!(
                        "Configured volume {} is missing; setup stopped without creating an empty replacement",
                        mount.volume
                    )
                })?;
        }
        let existing = self.optional_inspect("container", &env.container).await?;
        if existing.is_some() {
            let owned = self.owned(env).await?;
            if owned["Image"] != env.container_image() {
                bail!("Container image does not match");
            }
        } else {
            self.inspect("image", env.container_image()).await.context(
                "Container image is missing; setup stopped without changing the saved image ID",
            )?;
        }
        for (name, internal) in [(&env.private_network, true), (&env.egress_network, false)] {
            if let Some(info) = self.optional_inspect("network", name).await? {
                Self::check_network(env, &info, internal)?;
            }
        }
        let private = self.ensure_network(env, &env.private_network, true).await?;
        self.ensure_network(env, &env.egress_network, false).await?;
        let manager_id = manager_info["Id"].as_str().context("Missing manager ID")?;
        if manager_info["NetworkSettings"]["Networks"]
            .get(&env.private_network)
            .is_none()
        {
            self.docker(&[
                "network",
                "connect",
                private["Id"].as_str().context("Missing network ID")?,
                manager_id,
            ])
            .await?;
        }
        if let Some(info) = existing {
            let container_id = info["Id"].as_str().context("Missing container ID")?;
            if info["NetworkSettings"]["Networks"]
                .get(&env.private_network)
                .is_none()
            {
                self.connect_network(
                    env,
                    private["Id"].as_str().context("Missing network ID")?,
                    container_id,
                )
                .await?;
            }
            let mut saved = env.clone();
            self.set_internet(&mut saved, env.spec.internet).await?;
            if start && info["State"]["Running"] != true {
                // Also a container an earlier attempt created without its machine ID.
                self.write_machine_id(env).await?;
                self.hand_over_mount_parents(env).await?;
                self.docker(&["start", container_id])
                    .await
                    .map_err(|error| self.gpu_error(env, error))?;
            }
            return Ok(());
        }
        let args = self.container_args(env);
        self.docker_args(&args)
            .await
            .map_err(|error| self.gpu_error(env, error))?;
        self.write_machine_id(env).await?;
        self.hand_over_mount_parents(env).await?;
        if env.spec.internet {
            self.connect_network(env, &env.egress_network, &env.container)
                .await?;
        }
        if start {
            self.docker(&["start", &env.container])
                .await
                .map_err(|error| self.gpu_error(env, error))?;
        }
        Ok(())
    }
    /// The `docker create` arguments for a container: every setting that Docker
    /// fixes when it creates a container is here.
    pub(crate) fn container_args(&self, env: &Container) -> Vec<String> {
        let mut args: Vec<String> = vec![
            "create",
            "--name",
            &env.container,
            "--network",
            &env.private_network,
            "--restart",
            "unless-stopped",
            "--label",
            &format!("{LABEL}=true"),
            "--label",
            &format!("thoric.environment={}", env.id),
            "--shm-size",
            &format!("{}m", env.spec.shm_mb),
            "--tmpfs",
            "/run:rw,nosuid,size=128m",
            "--tmpfs",
            "/tmp:rw,nosuid,size=512m",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        if let Some(hostname) = &env.spec.hostname {
            args.extend(["--hostname".into(), hostname.clone()]);
        }
        if let Some(mac) = &env.spec.mac_address {
            // For the private network; the internet network gets it when it is connected.
            args.extend(["--mac-address".into(), mac.clone()]);
        }
        let envs = [
            ("DESKTOP_RESOLUTION", env.spec.resolution.clone()),
            ("DESKTOP_SUDO", env.spec.sudo.to_string()),
            ("DESKTOP_AUDIO", env.spec.audio.to_string()),
            ("DESKTOP_MICROPHONE", env.spec.microphone.to_string()),
            ("DESKTOP_GPU", env.spec.gpu.to_string()),
            ("GPU_ADAPTER", self.config.adapter.clone()),
        ];
        for (key, val) in envs {
            args.extend(["--env".into(), format!("{key}={val}")]);
        }
        if let Some(profile) = self.config.seccomp_profile() {
            // The CLI reads the file and sends its contents with the request.
            args.extend([
                "--security-opt".into(),
                format!("seccomp={}", profile.display()),
            ]);
        }
        if let Some(c) = env.spec.cpus {
            args.extend(["--cpus".into(), c.to_string()]);
        }
        if let Some(m) = env.spec.memory_mb {
            args.extend(["--memory".into(), format!("{m}m")]);
        }
        for m in env.mounts() {
            args.extend([
                "--mount".into(),
                format!(
                    "type=volume,src={},dst={}{}",
                    m.volume,
                    m.target,
                    if m.readonly { ",readonly" } else { "" }
                ),
            ]);
        }
        if env.spec.gpu {
            args.extend(crate::gpu::docker_args(
                &crate::gpu::routes(&self.config).used,
            ));
        }
        if env.spec.gamepad {
            // The virtual controller is created by a root helper inside the desktop; input
            // device nodes (major 13) are made there, so the cgroup rule must allow them.
            args.extend([
                "--device".into(),
                "/dev/uinput".into(),
                "--device-cgroup-rule".into(),
                "c 13:* rmw".into(),
            ]);
        }
        // Use an immutable ID, not a tag that could change between inspect and create.
        args.push(env.container_image().to_owned());
        args
    }
    pub async fn set_internet(&self, env: &mut Container, enabled: bool) -> Result<()> {
        let info = self.owned(env).await?;
        let connected = info["NetworkSettings"]["Networks"]
            .get(&env.egress_network)
            .is_some();
        let network = self.inspect("network", &env.egress_network).await?;
        Self::check_network(env, &network, false)?;
        let container_id = info["Id"].as_str().context("Missing container ID")?;
        let network_id = network["Id"].as_str().context("Missing network ID")?;
        if enabled && !connected {
            self.connect_network(env, network_id, container_id).await?;
        } else if !enabled && connected {
            self.docker(&["network", "disconnect", network_id, container_id])
                .await?;
        }
        env.spec.internet = enabled;
        self.db.put(CONTAINERS, &env.id, env)?;
        Ok(())
    }
    /// Delete a container and its networks. Named volumes stay, except with `volumes` those
    /// created for it that no other container mounts.
    pub(crate) async fn remove_container(
        &self,
        env: &Container,
        job_id: &str,
        volumes: bool,
    ) -> Result<RemovedVolumes> {
        self.append_log(
            job_id,
            &format!("Checking ownership before deleting \"{}\".", env.spec.name),
        )?;
        let container = match self.optional_inspect("container", &env.container).await? {
            Some(_) => Some(self.owned(env).await?),
            None => None,
        };
        let container_id = container.as_ref().and_then(|v| v["Id"].as_str());
        let manager_info = if let Some(name) = &self.config.manager {
            let m = self.inspect("container", name).await?;
            if m["Config"]["Labels"]["thoric.manager"] != "true" {
                bail!("Manager ownership does not match");
            }
            Some(m)
        } else {
            None
        };
        let manager = match &manager_info {
            Some(m) => Some(m["Id"].as_str().context("Missing manager ID")?.to_owned()),
            None => None,
        };
        let mut removed = RemovedVolumes::default();
        let mut doomed = Vec::new();
        if volumes {
            let mut owned = self.owned_volumes(&env.id).await?;
            // A label alone never hands over the manager's own volumes.
            owned.retain(|name| {
                !manager_info
                    .as_ref()
                    .is_some_and(|m| crate::volumes::manager_volume(m, name))
            });
            let others = self
                .db
                .list::<Container>(CONTAINERS)?
                .into_iter()
                .filter(|other| other.id != env.id)
                .collect::<Vec<_>>();
            let (free, shared) = crate::volumes::removable(owned, &others);
            for (name, other) in shared {
                self.append_log(
                    job_id,
                    &format!(
                        "Keeping the volume {name}: \"{}\" mounts it too.",
                        other.spec.name
                    ),
                )?;
                removed.kept.push(name);
            }
            doomed = free;
        }
        let mut networks = Vec::new();
        // Preflight ownership and every endpoint BEFORE deleting the container's writable layer.
        for (name, internal) in [(&env.private_network, true), (&env.egress_network, false)] {
            if let Some(info) = self.optional_inspect("network", name).await? {
                Self::check_network(env, &info, internal)?;
                if let Some(members) = info["Containers"].as_object() {
                    for id in members.keys() {
                        if Some(id.as_str()) != container_id && Some(id) != manager.as_ref() {
                            bail!("Network has another container; deletion stopped ({name})");
                        }
                    }
                }
                networks.push(info);
            }
        }
        if let Some(id) = container_id {
            self.docker(&["rm", "--force", id]).await?;
            self.append_log(job_id, "Removed the container and its writable layer.")?;
        } else {
            self.append_log(job_id, "Desktop container was already absent.")?;
        }
        self.remove_previous(env, job_id).await?;
        for info in networks {
            let id = info["Id"].as_str().context("Missing network ID")?;
            let role = if info["Internal"] == true {
                "private"
            } else {
                "internet"
            };
            if let Some(manager) = &manager
                && info["Containers"].get(manager).is_some()
            {
                self.docker(&["network", "disconnect", "--force", id, manager])
                    .await?;
                self.append_log(
                    job_id,
                    &format!("Disconnected the manager from the {role} network."),
                )?;
            }
            self.docker(&["network", "rm", id]).await?;
            self.append_log(job_id, &format!("Removed the container's {role} network."))?;
        }
        // The container is gone, so Docker refuses only a volume something else still uses.
        for name in doomed {
            match self.docker(&["volume", "rm", &name]).await {
                Ok(_) => {
                    self.append_log(job_id, &format!("Deleted the volume {name}."))?;
                    removed.deleted.push(name);
                }
                Err(error) => {
                    self.append_log(job_id, &format!("Keeping the volume {name}: {error:#}"))?;
                    removed.kept.push(name);
                }
            }
        }
        if let Some(image) = &env.working_image {
            self.release_working_image(image, job_id).await?;
        }
        self.remove_container_records(&env.id)?;
        self.forget_pinned(&env.id)?;
        self.db.remove(CONTAINERS, &env.id)?;
        crate::desktop_mcp::forget_container(self, &env.id)?;
        self.append_log(
            job_id,
            &if removed.deleted.is_empty() {
                format!(
                    "Deleted \"{}\". Named volumes and saved images were preserved.",
                    env.spec.name
                )
            } else {
                format!(
                    "Deleted \"{}\" and the volumes created for it. Other named volumes and saved images were preserved.",
                    env.spec.name
                )
            },
        )?;
        Ok(removed)
    }
    /// Remove records that describe processes inside a container that no longer exists.
    pub fn remove_container_records(&self, container_id: &str) -> Result<()> {
        for application in self.db.list::<Application>("application")? {
            if application.container_id == container_id {
                self.db.remove("application", &application.id)?;
            }
        }
        for recording in self.db.list::<Recording>("recording")? {
            if recording.container_id == container_id {
                self.db.remove("recording", &recording.id)?;
            }
        }
        for terminal in self.db.list::<Terminal>("terminal")? {
            if terminal.container_id == container_id {
                self.db.remove("terminal", &terminal.id)?;
            }
        }
        Ok(())
    }
    /// Discard the container's writable layer and recreate it from the pinned source image.
    pub async fn reset_container(&self, id: &str, job_id: &str) -> Result<Value> {
        let mut env = self.container(id).map_err(|e| anyhow::anyhow!(e.1))?;
        let manager = self
            .config
            .manager
            .as_deref()
            .context("MANAGER_CONTAINER is required")?;
        let info = self.inspect("container", manager).await?;
        if info["Config"]["Labels"]["thoric.manager"] != "true" {
            bail!("Manager label does not match");
        }
        crate::volumes::refuse_manager_volumes(&info, &env.mounts())?;
        // Check the image before touching the container, so a missing image leaves the desktop intact.
        self.source_image(&mut env, job_id)
            .await
            .context("Source image is missing; the container was left unchanged")?;
        let existing = match self.optional_inspect("container", &env.container).await? {
            Some(_) => Some(self.owned(&env).await?),
            None => None,
        };
        // The reset keeps the desktop's identity, also one it had before identities were kept.
        crate::identity::adopt(&mut env, existing.as_ref());
        if let Some(owned) = existing {
            self.docker(&[
                "rm",
                "--force",
                owned["Id"].as_str().context("Missing container ID")?,
            ])
            .await?;
            self.append_log(
                job_id,
                "Removed the container and its writable layer. Named volumes were preserved.",
            )?;
        } else {
            self.append_log(job_id, "Desktop container was already absent.")?;
        }
        // A reset starts over from the source image, so files carried across settings changes go too.
        self.remove_previous(&env, job_id).await?;
        if let Some(image) = env.working_image.take() {
            self.release_working_image(&image, job_id).await?;
        }
        self.remove_container_records(id)?;
        crate::compose::setup_again(&mut env);
        let result = self.provision(&env, manager).await;
        env.last_error = result.as_ref().err().map(|e| format!("{e:#}"));
        self.db.put(CONTAINERS, id, &env)?;
        result?;
        self.append_log(
            job_id,
            "Recreated the container from its saved source image.",
        )?;
        self.run_setup(id, job_id).await?;
        Ok(json!({"container_id": id, "volumes_preserved": true, "images_preserved": true}))
    }
    pub async fn address(&self, env: &Container) -> Result<String> {
        let info = self.owned(env).await?;
        if info["State"]["Running"] != true {
            bail!("Container is not running");
        }
        Ok(format!("{}:5900", private_ip(env, &info)?))
    }
    pub fn append_log(&self, id: &str, line: &str) -> Result<()> {
        let mut job: Job = self.db.get("job", id)?;
        job.log.push_str(line);
        job.log.push('\n');
        job.trim_log();
        self.db.put("job", id, &job)
    }
    /// Replace the job's measured progress; `None` once the measured step is over.
    pub fn set_progress(&self, id: &str, progress: Option<Value>) -> Result<()> {
        let mut job: Job = self.db.get("job", id)?;
        job.progress = progress;
        self.db.put("job", id, &job)?;
        self.events.progress(id, job.progress.as_ref());
        Ok(())
    }
    pub async fn docker_logged(&self, id: &str, args: &[String]) -> Result<()> {
        self.docker_logged_input(id, args, None).await
    }
    pub async fn docker_logged_input(
        &self,
        id: &str,
        args: &[String],
        input: Option<Vec<u8>>,
    ) -> Result<()> {
        self.run_logged(id, args, input, false, LOGGED_LIMIT).await
    }
    /// A logged command that may run longer than the usual hour, such as a compose setup step
    /// whose own timeout is longer.
    pub async fn docker_logged_within(
        &self,
        id: &str,
        args: &[String],
        input: Option<Vec<u8>>,
        limit: Duration,
    ) -> Result<()> {
        self.run_logged(id, args, input, false, limit).await
    }
    /// A script build's `docker build`: its output is logged a whole line at a time, and a line
    /// that begins a build stage starts it.
    pub async fn docker_build_logged(&self, id: &str, args: &[String]) -> Result<()> {
        self.run_logged(id, args, None, true, LOGGED_LIMIT).await
    }
    async fn run_logged(
        &self,
        id: &str,
        args: &[String],
        input: Option<Vec<u8>>,
        staged: bool,
        limit: Duration,
    ) -> Result<()> {
        let mut child = self
            .command(args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let out = child.stdout.take().unwrap();
        let err = child.stderr.take().unwrap();
        let stdin = child.stdin.take();
        let write = async move {
            // Scripts may exit before reading all of their input; the exit status is authoritative.
            if let (Some(mut stdin), Some(input)) = (stdin, input)
                && let Err(e) = async {
                    stdin.write_all(&input).await?;
                    stdin.shutdown().await
                }
                .await
            {
                tracing::debug!("Job input was not fully delivered: {e}");
            }
            anyhow::Ok(())
        };
        let read = async {
            let (mut out, mut err) = (out, err);
            let (mut a, mut b) = (false, false);
            let (mut dec_a, mut dec_b) = (
                crate::process::TextDecoder::default(),
                crate::process::TextDecoder::default(),
            );
            let (mut buf_a, mut buf_b) = ([0; 8192], [0; 8192]);
            let mut lines = staged.then(<[crate::stages::Lines; 2]>::default);
            let mut log = |stream: usize, text: String, end: bool| match &mut lines {
                Some(lines) => {
                    let mut whole = lines[stream].push(&text);
                    if end {
                        whole.push_str(&lines[stream].finish());
                    }
                    self.log_build_output(id, &whole)
                }
                None if text.is_empty() => Ok(()),
                None => self.append_log(id, &text),
            };
            while !a || !b {
                tokio::select! {
                    n = out.read(&mut buf_a), if !a => { let n=n?; a=n==0; let text=if a {dec_a.finish()} else {dec_a.push(&buf_a[..n])}; log(0,text,a)?; },
                    n = err.read(&mut buf_b), if !b => { let n=n?; b=n==0; let text=if b {dec_b.finish()} else {dec_b.push(&buf_b[..n])}; log(1,text,b)?; },
                }
            }
            let status = child.wait().await?;
            if !status.success() {
                self.append_log(id, &format!("Docker command exited with {status}."))?;
                bail!("Docker operation failed ({status}); check the job log");
            }
            anyhow::Ok(())
        };
        let result = tokio::time::timeout(limit, async {
            tokio::try_join!(write, read)?;
            anyhow::Ok(())
        })
        .await;
        if !matches!(result, Ok(Ok(()))) {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        result.with_context(|| match limit.as_secs() {
            3600 => "Image preparation exceeded the one-hour limit".to_owned(),
            seconds => format!("The operation exceeded its {}-minute limit", seconds / 60),
        })??;
        Ok(())
    }
    pub async fn cleanup_build(&self, id: &str) -> Result<()> {
        if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("Invalid build ID");
        }
        let dir = self.config.data.join("builds").join(id);
        match tokio::fs::remove_dir_all(&dir).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        let tag = format!("thoric/build-base:{id}");
        let names = self
            .docker(&["image", "ls", "--format", "{{.Repository}}:{{.Tag}}", &tag])
            .await?;
        if String::from_utf8_lossy(&names)
            .lines()
            .any(|name| name == tag)
        {
            self.docker(&["image", "rm", &tag]).await?;
        }
        Ok(())
    }
}

/// An image `VOLUME` path needs an explicit named volume, or snapshots would silently exclude it.
pub(crate) fn image_volumes_mounted(image: &Value, mounts: &[Mount]) -> Result<()> {
    if let Some(volumes) = image["Config"]["Volumes"].as_object() {
        for target in volumes.keys() {
            if !mounts.iter().any(|m| m.target == *target) {
                bail!(
                    "Choose a named volume for the image's {target} VOLUME path; otherwise snapshots would exclude its data"
                );
            }
        }
    }
    Ok(())
}
/// The desktop's address on its private network, which only the manager shares with it, from
/// `docker inspect` of its container.
pub(crate) fn private_ip(env: &Container, info: &Value) -> Result<std::net::Ipv4Addr> {
    info["NetworkSettings"]["Networks"][&env.private_network]["IPAddress"]
        .as_str()
        .context("Missing private network address")?
        .parse()
        .context("Invalid Docker IP address")
}
