use crate::{App, model::*};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};

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
    pub async fn owned(&self, env: &Environment) -> Result<Value> {
        let info = self.inspect("container", &env.container).await?;
        if info["Config"]["Labels"][LABEL] != "true"
            || info["Config"]["Labels"]["io.wsl-desktop.environment"] != env.id
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
    pub fn check_network(env: &Environment, info: &Value, internal: bool) -> Result<()> {
        if info["Labels"][LABEL] != "true"
            || info["Labels"]["io.wsl-desktop.environment"] != env.id
            || info["Internal"] != internal
            || info["Driver"] != "bridge"
        {
            bail!("Network ownership or type does not match");
        }
        Ok(())
    }
    async fn ensure_network(&self, env: &Environment, name: &str, internal: bool) -> Result<Value> {
        if let Some(info) = self.optional_inspect("network", name).await? {
            Self::check_network(env, &info, internal)?;
            return Ok(info);
        }
        let mut args = vec![
            "network",
            "create",
            "--label",
            "io.wsl-desktop.managed=true",
            "--label",
        ];
        let owner = format!("io.wsl-desktop.environment={}", env.id);
        args.push(&owner);
        if internal {
            args.push("--internal");
        }
        args.push(name);
        let id = self.docker(&args).await?;
        self.inspect("network", String::from_utf8_lossy(&id).trim())
            .await
    }
    pub fn environment(&self, id: &str) -> crate::error::Result<Environment> {
        self.db
            .get("environment", id)
            .map_err(crate::error::Error::from)
    }
    pub async fn create_environment(&self, id: &str, spec: EnvironmentSpec) -> Result<Value> {
        spec.validate()?;
        if spec.audio && !self.config.audio {
            bail!("Audio is disabled for this installation");
        }
        if spec.microphone && !self.config.microphone {
            bail!("Microphone is disabled for this installation");
        }
        let manager = self
            .config
            .manager
            .as_ref()
            .context("Set MANAGER_CONTAINER; the service must run in Docker")?;
        let manager_info = self.inspect("container", manager).await?;
        if manager_info["Config"]["Labels"]["io.wsl-desktop.manager"] != "true" {
            bail!("MANAGER_CONTAINER does not point to the manager");
        }
        let image = self
            .inspect("image", &spec.image)
            .await
            .context("Image not available. Build or pull it first")?;
        if image["Config"]["Labels"]["io.wsl-desktop.desktop"] != "true" {
            bail!("Image must be derived from a Bilskirnir desktop image");
        }
        if let Some(volumes) = image["Config"]["Volumes"].as_object() {
            for target in volumes.keys() {
                if !spec.mounts.iter().any(|m| m.target == *target) {
                    bail!(
                        "Choose a named volume for the image's {target} VOLUME path; otherwise snapshots would exclude its data"
                    );
                }
            }
        }
        for m in &spec.mounts {
            self.inspect("volume", &m.volume)
                .await
                .context("Volume not found")?;
        }
        let env = Environment {
            id: id.into(),
            container: format!("wsld-{id}"),
            private_network: format!("wsld-{id}-private"),
            egress_network: format!("wsld-{id}-egress"),
            source_image_id: image["Id"].as_str().context("Missing image ID")?.into(),
            spec,
            created_at: now(),
            last_error: None,
            desktop: crate::profiles::from_image(&image),
        };
        // Save intent first: failed setup and interrupted jobs remain discoverable and removable.
        self.db.put("environment", id, &env)?;
        let result = self.provision(&env, manager).await;
        if let Err(ref e) = result {
            let mut failed = env.clone();
            failed.last_error = Some(format!("{e:#}"));
            self.db.put("environment", id, &failed)?;
            // Keep intent and any partial resources for explicit retry/removal; never touch unrelated objects.
        }
        result?;
        Ok(json!({"environment_id":id}))
    }
    pub async fn retry_environment(&self, id: &str) -> Result<Value> {
        let mut env = self.environment(id).map_err(|e| anyhow::anyhow!(e.1))?;
        let manager = self
            .config
            .manager
            .as_deref()
            .context("MANAGER_CONTAINER is required")?;
        let info = self.inspect("container", manager).await?;
        if info["Config"]["Labels"]["io.wsl-desktop.manager"] != "true" {
            bail!("Manager label does not match");
        }
        let result = self.provision(&env, manager).await;
        env.last_error = result.as_ref().err().map(|e| format!("{e:#}"));
        self.db.put("environment", id, &env)?;
        result?;
        Ok(json!({"environment_id":id}))
    }
    async fn provision(&self, env: &Environment, manager: &str) -> Result<()> {
        // Validate all existing resources before creating or starting anything.
        env.spec.validate()?;
        for mount in &env.spec.mounts {
            self.inspect("volume", &mount.volume).await.context(
                "Configured volume is missing; setup stopped without creating an empty replacement",
            )?;
        }
        let existing = self.optional_inspect("container", &env.container).await?;
        if existing.is_some() {
            let owned = self.owned(env).await?;
            if owned["Image"] != env.source_image_id {
                bail!("Container source image does not match");
            }
        } else {
            self.inspect("image", &env.source_image_id).await.context(
                "Source image is missing; setup stopped without changing the saved image ID",
            )?;
        }
        for (name, internal) in [(&env.private_network, true), (&env.egress_network, false)] {
            if let Some(info) = self.optional_inspect("network", name).await? {
                Self::check_network(env, &info, internal)?;
            }
        }
        let private = self.ensure_network(env, &env.private_network, true).await?;
        self.ensure_network(env, &env.egress_network, false).await?;
        let manager_info = self.inspect("container", manager).await?;
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
                self.docker(&[
                    "network",
                    "connect",
                    private["Id"].as_str().unwrap(),
                    container_id,
                ])
                .await?;
            }
            let mut saved = env.clone();
            self.set_internet(&mut saved, env.spec.internet).await?;
            if info["State"]["Running"] != true {
                self.docker(&["start", container_id]).await?;
            }
            return Ok(());
        }
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
            &format!("io.wsl-desktop.environment={}", env.id),
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
        if let Some(c) = env.spec.cpus {
            args.extend(["--cpus".into(), c.to_string()]);
        }
        if let Some(m) = env.spec.memory_mb {
            args.extend(["--memory".into(), format!("{m}m")]);
        }
        for m in &env.spec.mounts {
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
            if self.config.cuda {
                args.extend(["--gpus".into(), "all".into()]);
            }
            if self.config.wsl_gpu {
                args.extend([
                    "--device".into(),
                    "/dev/dxg".into(),
                    "--mount".into(),
                    "type=bind,src=/usr/lib/wsl,dst=/usr/lib/wsl,readonly".into(),
                ]);
            }
        }
        // Use immutable ID, not a tag that could change between inspect and create.
        args.push(env.source_image_id.clone());
        self.docker_args(&args).await?;
        if env.spec.internet {
            self.docker(&["network", "connect", &env.egress_network, &env.container])
                .await?;
        }
        self.docker(&["start", &env.container]).await?;
        Ok(())
    }
    pub async fn set_internet(&self, env: &mut Environment, enabled: bool) -> Result<()> {
        let info = self.owned(env).await?;
        let connected = info["NetworkSettings"]["Networks"]
            .get(&env.egress_network)
            .is_some();
        let network = self.inspect("network", &env.egress_network).await?;
        Self::check_network(env, &network, false)?;
        let container_id = info["Id"].as_str().context("Missing container ID")?;
        let network_id = network["Id"].as_str().context("Missing network ID")?;
        if enabled != connected {
            self.docker(&[
                "network",
                if enabled { "connect" } else { "disconnect" },
                network_id,
                container_id,
            ])
            .await?;
        }
        env.spec.internet = enabled;
        self.db.put("environment", &env.id, env)?;
        Ok(())
    }
    pub async fn remove_environment(&self, env: &Environment, job_id: &str) -> Result<()> {
        self.append_log(
            job_id,
            &format!("Checking ownership before deleting \"{}\".", env.spec.name),
        )?;
        let container = match self.optional_inspect("container", &env.container).await? {
            Some(_) => Some(self.owned(env).await?),
            None => None,
        };
        let container_id = container.as_ref().and_then(|v| v["Id"].as_str());
        let manager = if let Some(name) = &self.config.manager {
            let m = self.inspect("container", name).await?;
            if m["Config"]["Labels"]["io.wsl-desktop.manager"] != "true" {
                bail!("Manager ownership does not match");
            }
            Some(m["Id"].as_str().context("Missing manager ID")?.to_owned())
        } else {
            None
        };
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
            self.append_log(
                job_id,
                "Removed the desktop container and its writable layer.",
            )?;
        } else {
            self.append_log(job_id, "Desktop container was already absent.")?;
        }
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
            self.append_log(
                job_id,
                &format!("Removed the environment's {role} network."),
            )?;
        }
        self.db.remove("environment", &env.id)?;
        self.append_log(
            job_id,
            &format!(
                "Deleted \"{}\". Named volumes and saved images were preserved.",
                env.spec.name
            ),
        )?;
        Ok(())
    }
    pub async fn address(&self, env: &Environment) -> Result<String> {
        let info = self.owned(env).await?;
        if info["State"]["Running"] != true {
            bail!("Environment is not running");
        }
        let ip = info["NetworkSettings"]["Networks"][&env.private_network]["IPAddress"]
            .as_str()
            .context("Missing private network address")?;
        ip.parse::<std::net::Ipv4Addr>()
            .context("Invalid Docker IP address")?;
        Ok(format!("{ip}:5900"))
    }
    pub fn append_log(&self, id: &str, line: &str) -> Result<()> {
        let mut job: Job = self.db.get("job", id)?;
        job.log.push_str(line);
        job.log.push('\n');
        job.trim_log();
        self.db.put("job", id, &job)
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
            if let (Some(mut stdin), Some(input)) = (stdin, input) {
                stdin.write_all(&input).await?;
                stdin.shutdown().await?;
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
            while !a || !b {
                tokio::select! {
                    n = out.read(&mut buf_a), if !a => { let n=n?; a=n==0; let text=if a {dec_a.finish()} else {dec_a.push(&buf_a[..n])}; if !text.is_empty() { self.append_log(id,&text)?; } },
                    n = err.read(&mut buf_b), if !b => { let n=n?; b=n==0; let text=if b {dec_b.finish()} else {dec_b.push(&buf_b[..n])}; if !text.is_empty() { self.append_log(id,&text)?; } },
                }
            }
            let status = child.wait().await?;
            if !status.success() {
                self.append_log(id, &format!("Docker command exited with {status}."))?;
                bail!("Docker operation failed ({status}); check the job log");
            }
            anyhow::Ok(())
        };
        let result = tokio::time::timeout(Duration::from_secs(3600), async {
            tokio::try_join!(write, read)?;
            anyhow::Ok(())
        })
        .await;
        if !matches!(result, Ok(Ok(()))) {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        result.context("Image preparation exceeded the one-hour limit")??;
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
        // Legacy tags are retained only for cleanup of jobs created before the rename.
        for namespace in ["bilskirnir", "wsl-desktop"] {
            let tag = format!("{namespace}/build-base:{id}");
            let names = self
                .docker(&["image", "ls", "--format", "{{.Repository}}:{{.Tag}}", &tag])
                .await?;
            if String::from_utf8_lossy(&names)
                .lines()
                .any(|name| name == tag)
            {
                self.docker(&["image", "rm", &tag]).await?;
            }
        }
        Ok(())
    }
}
