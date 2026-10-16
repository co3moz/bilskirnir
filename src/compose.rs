//! Image composes: parameters asked for when a container is created from a saved image, and
//! a setup script that receives them as environment variables once the new desktop is ready.
//!
//! A compose belongs to an image record. A new container pins a copy of it together with the
//! values it was given, so editing the image later changes neither what the container ran nor
//! what a reset runs again. The values live in a record of their own that no route returns:
//! the container shows the values of ordinary parameters and only the names of secret ones,
//! and the setup job's log masks every secret value.
mod parameters;

use crate::{
    App,
    error::{Error, Result},
    model::*,
};
use anyhow::{Context, bail};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
pub(crate) use parameters::Parameter;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

const RUNNER: &str = include_str!("setup_runner.py");
/// Gives the desktop user the folders of the volume parameters; runs as root.
const PATHS: &str = include_str!("setup_paths.py");
/// The database kind of each container's pinned compose and parameter values.
const PINNED: &str = "environment-setup";
const PARAMETER_LIMIT: usize = 32;
const SCRIPT_LIMIT: usize = 1_000_000;
/// How long a new desktop may take to report ready before its setup is given up.
const READY_LIMIT: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Compose {
    /// Asked for when a container is created; each one is an environment variable of the script.
    #[serde(default)]
    pub parameters: Vec<Parameter>,
    /// A bash script, run with fail-on-error and stdin closed.
    pub script: String,
    /// Run the script as root instead of the desktop user.
    #[serde(default)]
    pub root: bool,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}
fn default_timeout() -> u64 {
    600
}

impl Compose {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.script.trim().is_empty()
            || self.script.len() > SCRIPT_LIMIT
            || self.script.contains('\0')
        {
            bail!("A compose needs a setup script of 1 to 1000000 bytes without NUL characters");
        }
        if !(1..=3500).contains(&self.timeout_seconds) {
            bail!("timeout_seconds must be between 1 and 3500");
        }
        if self.parameters.len() > PARAMETER_LIMIT {
            bail!("A compose has at most {PARAMETER_LIMIT} parameters");
        }
        let mut names = HashSet::new();
        let mut paths = HashSet::new();
        for parameter in &self.parameters {
            parameter.validate()?;
            if !names.insert(parameter.name.as_str()) {
                bail!("Parameter names must be unique: {}", parameter.name);
            }
            if let Some(variable) = parameter.volume_variable() {
                if !paths.insert(parameter.path.as_str()) {
                    bail!(
                        "Volume parameters need paths of their own: {} is used twice",
                        parameter.path
                    );
                }
                if self.parameters.iter().any(|p| p.name == variable) {
                    bail!(
                        "{variable} carries the volume of {}, so no parameter can have that name",
                        parameter.name
                    );
                }
            }
        }
        Ok(())
    }

    /// The volumes a container with these values mounts: each volume parameter's volume at its
    /// path, for those given one.
    pub fn mounts(&self, values: &BTreeMap<String, String>) -> Vec<Mount> {
        self.parameters
            .iter()
            .filter_map(|p| p.mount(values.get(&p.name)?))
            .collect()
    }

    /// Every parameter's value for a new container: the one given, otherwise the default.
    pub fn resolve(
        &self,
        given: &BTreeMap<String, String>,
    ) -> anyhow::Result<BTreeMap<String, String>> {
        if let Some(name) = given
            .keys()
            .find(|name| !self.parameters.iter().any(|p| &p.name == *name))
        {
            bail!("The image's compose has no parameter {name}");
        }
        let mut values = BTreeMap::new();
        for parameter in &self.parameters {
            let value = parameter.resolve(given.get(&parameter.name))?;
            values.insert(parameter.name.clone(), value);
        }
        Ok(values)
    }
}

/// Check a compose's volumes against the container's own volume settings: a path the compose
/// mounts a volume at gets no second one, and a volume it mounts is mounted nowhere else, so
/// one folder of the desktop does not show up under two paths.
pub(crate) fn check_mounts(own: &[Mount], compose: &[Mount]) -> anyhow::Result<()> {
    for (index, mount) in compose.iter().enumerate() {
        if own.iter().any(|other| other.target == mount.target) {
            bail!(
                "The image's compose mounts the volume {} at {}; remove the volume setting with that path",
                mount.volume,
                mount.target
            );
        }
        if let Some(other) = own
            .iter()
            .chain(&compose[..index])
            .find(|other| other.volume == mount.volume)
        {
            bail!(
                "The volume {} would be mounted at both {} and {}; choose another volume for one of them",
                mount.volume,
                other.target,
                mount.target
            );
        }
    }
    Ok(())
}

/// Where a container's compose setup stands, as its record shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Setup {
    /// `pending`, `running`, `completed` or `failed`.
    pub status: String,
    /// The image record whose compose the container was created with.
    pub image: String,
    /// The values of the parameters that are not secret.
    pub parameters: BTreeMap<String, String>,
    /// The names of the secret parameters.
    pub secrets: Vec<String>,
    #[serde(default)]
    pub finished_at: Option<u64>,
    /// The volumes the compose's volume parameters were given, mounted beside the container's
    /// own every time it is created.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mounts: Vec<Mount>,
}

/// The compose a container was created with and every value it was given, secret ones too.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Pinned {
    /// Saved as `template` before templates were called composes.
    #[serde(alias = "template")]
    pub compose: Compose,
    pub values: BTreeMap<String, String>,
}
impl Pinned {
    /// What the container record shows of it.
    pub fn setup(&self, image: &str) -> Setup {
        let secret = |name: &String| {
            self.compose
                .parameters
                .iter()
                .any(|p| &p.name == name && p.secret)
        };
        Setup {
            status: "pending".into(),
            image: image.into(),
            parameters: self
                .values
                .iter()
                .filter(|(name, _)| !secret(name))
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
            secrets: self
                .values
                .keys()
                .filter(|name| secret(name))
                .cloned()
                .collect(),
            finished_at: None,
            mounts: self.mounts(),
        }
    }
    pub fn mounts(&self) -> Vec<Mount> {
        self.compose.mounts(&self.values)
    }
    /// The paths of the volume parameters, which the desktop user is given before the script
    /// runs, whether a volume was mounted there or not.
    pub fn paths(&self) -> Vec<&str> {
        self.compose
            .parameters
            .iter()
            .filter(|p| p.volume_variable().is_some())
            .map(|p| p.path.as_str())
            .collect()
    }
    /// The script's variables: every parameter's value, a volume parameter's as its path and
    /// volume.
    pub fn environment(&self) -> BTreeMap<String, String> {
        self.compose
            .parameters
            .iter()
            .filter_map(|p| Some(p.environment(self.values.get(&p.name)?)))
            .flatten()
            .collect()
    }
    /// The setup runner's request on stdin, which keeps values out of command lines.
    pub fn request(&self) -> Value {
        let secrets = self
            .compose
            .parameters
            .iter()
            .filter(|p| p.secret)
            .map(|p| &p.name)
            .collect::<Vec<_>>();
        json!({
            "script": self.compose.script.replace("\r\n", "\n"),
            "parameters": self.environment(),
            "secrets": secrets,
            "timeout_seconds": self.compose.timeout_seconds,
        })
    }
}

/// The image record whose compose applies to a new container from `requested`, a reference
/// or image ID that Docker resolved to `image_id`. Records of one image share the newest compose.
pub(crate) fn compose_image<'a>(
    records: &'a [ImageRecord],
    requested: &str,
    image_id: &str,
) -> Option<&'a ImageRecord> {
    records.iter().find(|record| {
        record.compose.is_some()
            && (record.image_id == image_id
                || record.image_id == requested
                || record.reference == requested)
    })
}

impl App {
    /// The compose and values a new container from this image runs, or `None` without one.
    pub(crate) fn pin_compose(
        &self,
        requested: &str,
        image_id: &str,
        given: &BTreeMap<String, String>,
    ) -> anyhow::Result<Option<(String, Pinned)>> {
        let records: Vec<ImageRecord> = self.db.list("image")?;
        let Some(record) = compose_image(&records, requested, image_id) else {
            if let Some(name) = given.keys().next() {
                bail!("The image has no compose, so it takes no parameter {name}");
            }
            return Ok(None);
        };
        let compose = record.compose.clone().context("Missing compose")?;
        let values = compose.resolve(given)?;
        Ok(Some((record.id.clone(), Pinned { compose, values })))
    }

    /// The compose a container was created with, which a snapshot of it keeps.
    pub(crate) fn pinned_compose(&self, container_id: &str) -> Option<Compose> {
        self.db
            .get::<Pinned>(PINNED, container_id)
            .ok()
            .map(|pinned| pinned.compose)
    }

    pub(crate) fn save_pinned(&self, container_id: &str, pinned: &Pinned) -> anyhow::Result<()> {
        self.db.put(PINNED, container_id, pinned)
    }

    pub(crate) fn forget_pinned(&self, container_id: &str) -> anyhow::Result<()> {
        self.db.remove(PINNED, container_id)
    }

    /// Run the compose setup of a container whose setup has not completed. A failure is
    /// recorded on the container, which keeps running so the outcome can be inspected.
    pub(crate) async fn run_setup(&self, id: &str, job_id: &str) -> anyhow::Result<()> {
        let mut env: Container = self.db.get(CONTAINERS, id)?;
        match &env.setup {
            Some(setup) if setup.status != "completed" => {}
            _ => return Ok(()),
        }
        let pinned: Pinned = self
            .db
            .get(PINNED, id)
            .context("The compose values of this container are missing; create it again")?;
        set_status(&mut env, "running", None);
        env.last_error = None;
        self.db.put(CONTAINERS, id, &env)?;
        let outcome = self.execute_setup(&env, &pinned, job_id).await;
        let mut env: Container = self.db.get(CONTAINERS, id)?;
        set_status(
            &mut env,
            if outcome.is_ok() {
                "completed"
            } else {
                "failed"
            },
            Some(now()),
        );
        env.last_error = outcome.as_ref().err().map(|e| format!("{e:#}"));
        self.db.put(CONTAINERS, id, &env)?;
        outcome
    }

    async fn execute_setup(
        &self,
        env: &Container,
        pinned: &Pinned,
        job_id: &str,
    ) -> anyhow::Result<()> {
        self.append_log(
            job_id,
            "Waiting for the desktop before running the compose setup.",
        )?;
        let container = self.wait_ready(env).await?;
        let paths = pinned.paths();
        if !paths.is_empty() {
            self.append_log(
                job_id,
                &format!("Preparing {} for the desktop user.", paths.join(", ")),
            )?;
            let mut args: Vec<String> = ["exec", "--user", "0:0", &container, "python3", "-c"]
                .map(str::to_owned)
                .to_vec();
            args.push(PATHS.into());
            args.extend(paths.iter().map(|path| path.to_string()));
            self.docker_logged(job_id, &args)
                .await
                .context("The folders of the compose's volume parameters could not be prepared")?;
        }
        let root = pinned.compose.root;
        let user = if root { "root" } else { "1001:1001" };
        let names = pinned.environment().into_keys().collect::<Vec<_>>();
        self.append_log(
            job_id,
            &format!(
                "Running the compose setup as {user} (timeout: {} seconds). Parameters: {}.",
                pinned.compose.timeout_seconds,
                if names.is_empty() {
                    "none".into()
                } else {
                    names.join(", ")
                }
            ),
        )?;
        let mut args = crate::exec::desktop_exec(user, root);
        args.extend([container, "python3".into(), "-c".into(), RUNNER.into()]);
        let input = serde_json::to_vec(&pinned.request())?;
        self.docker_logged_input(job_id, &args, Some(input))
            .await
            .context("The compose setup failed")?;
        self.append_log(job_id, "Compose setup completed.")?;
        Ok(())
    }

    /// Wait until the desktop session reports ready and return the container ID.
    async fn wait_ready(&self, env: &Container) -> anyhow::Result<String> {
        let started = Instant::now();
        loop {
            let info = self.owned(env).await?;
            let id = info["Id"]
                .as_str()
                .context("Missing container ID")?
                .to_owned();
            let state = &info["State"];
            let running = state["Running"] == true && state["Restarting"] != true;
            if running
                && self
                    .docker(&[
                        "exec",
                        "--user",
                        "1001:1001",
                        &id,
                        "test",
                        "-e",
                        "/run/desktop/ready",
                    ])
                    .await
                    .is_ok()
            {
                return Ok(id);
            }
            if state["Running"] != true && state["Restarting"] != true {
                bail!("The container stopped before the compose setup could run; check its logs");
            }
            if started.elapsed() > READY_LIMIT {
                bail!(
                    "The desktop was not ready within {} seconds, so the compose setup did not run; check its logs",
                    READY_LIMIT.as_secs()
                );
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

fn set_status(env: &mut Container, status: &str, finished_at: Option<u64>) {
    if let Some(setup) = env.setup.as_mut() {
        setup.status = status.into();
        setup.finished_at = finished_at;
    }
}

/// A new container made from the source image lacks what the setup changed, so it runs again.
pub(crate) fn setup_again(env: &mut Container) {
    set_status(env, "pending", None);
}

/// No job survives a restart, so a setup still waiting or running then will not finish on its
/// own; retry runs it again.
pub(crate) fn mark_interrupted(db: &crate::db::Db) -> anyhow::Result<()> {
    for mut env in db.list::<Container>(CONTAINERS)? {
        if env
            .setup
            .as_ref()
            .is_some_and(|setup| setup.status == "pending" || setup.status == "running")
        {
            set_status(&mut env, "failed", Some(now()));
            // An error that stopped the setup earlier explains more than the restart does.
            env.last_error.get_or_insert_with(|| {
                "The manager restarted before the compose setup finished; retry to run it again"
                    .into()
            });
            db.put(CONTAINERS, &env.id, &env)?;
        }
    }
    Ok(())
}

pub async fn get(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Compose>> {
    let record: ImageRecord = app.db.get("image", &id)?;
    record
        .compose
        .map(Json)
        .ok_or_else(|| Error(StatusCode::NOT_FOUND, "This image has no compose".into()))
}

pub async fn set(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(compose): Json<Compose>,
) -> Result<Json<Value>> {
    compose.validate()?;
    replace(&app, &id, Some(compose)).await
}

pub async fn remove(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    replace(&app, &id, None).await
}

async fn replace(app: &App, id: &str, compose: Option<Compose>) -> Result<Json<Value>> {
    let lock = app.resource_lock(&format!("image:{id}"));
    let _guard = lock.lock().await;
    let mut record: ImageRecord = app.db.get("image", id)?;
    record.compose = compose;
    app.db.put("image", id, &record)?;
    Ok(Json(crate::api::image_summary(&record)))
}

#[cfg(test)]
#[path = "compose_tests.rs"]
mod tests;
