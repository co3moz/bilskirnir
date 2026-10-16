//! Image templates: parameters asked for when a container is created from a saved image, and
//! a setup script that receives them as environment variables once the new desktop is ready.
//!
//! A template belongs to an image record. A new container pins a copy of it together with the
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
/// The database kind of each templated container's pinned template and parameter values.
const PINNED: &str = "environment-setup";
const PARAMETER_LIMIT: usize = 32;
const SCRIPT_LIMIT: usize = 1_000_000;
/// How long a new desktop may take to report ready before its setup is given up.
const READY_LIMIT: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Template {
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

impl Template {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.script.trim().is_empty()
            || self.script.len() > SCRIPT_LIMIT
            || self.script.contains('\0')
        {
            bail!("A template needs a setup script of 1 to 1000000 bytes without NUL characters");
        }
        if !(1..=3500).contains(&self.timeout_seconds) {
            bail!("timeout_seconds must be between 1 and 3500");
        }
        if self.parameters.len() > PARAMETER_LIMIT {
            bail!("A template has at most {PARAMETER_LIMIT} parameters");
        }
        let mut names = HashSet::new();
        for parameter in &self.parameters {
            parameter.validate()?;
            if !names.insert(parameter.name.as_str()) {
                bail!("Parameter names must be unique: {}", parameter.name);
            }
        }
        Ok(())
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
            bail!("The image's template has no parameter {name}");
        }
        let mut values = BTreeMap::new();
        for parameter in &self.parameters {
            let value = parameter.resolve(given.get(&parameter.name))?;
            values.insert(parameter.name.clone(), value);
        }
        Ok(values)
    }
}

/// Where a container's template setup stands, as its record shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Setup {
    /// `pending`, `running`, `completed` or `failed`.
    pub status: String,
    /// The image record whose template the container was created with.
    pub image: String,
    /// The values of the parameters that are not secret.
    pub parameters: BTreeMap<String, String>,
    /// The names of the secret parameters.
    pub secrets: Vec<String>,
    #[serde(default)]
    pub finished_at: Option<u64>,
}

/// The template a container was created with and every value it was given, secret ones too.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Pinned {
    pub template: Template,
    pub values: BTreeMap<String, String>,
}
impl Pinned {
    /// What the container record shows of it.
    pub fn setup(&self, image: &str) -> Setup {
        let secret = |name: &String| {
            self.template
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
        }
    }
    /// The setup runner's request on stdin, which keeps values out of command lines.
    pub fn request(&self) -> Value {
        let secrets = self
            .template
            .parameters
            .iter()
            .filter(|p| p.secret)
            .map(|p| &p.name)
            .collect::<Vec<_>>();
        json!({
            "script": self.template.script.replace("\r\n", "\n"),
            "parameters": self.values,
            "secrets": secrets,
            "timeout_seconds": self.template.timeout_seconds,
        })
    }
}

/// The image record whose template applies to a new container from `requested`, a reference
/// or image ID that Docker resolved to `image_id`. Records of one image share the newest template.
pub(crate) fn template_image<'a>(
    records: &'a [ImageRecord],
    requested: &str,
    image_id: &str,
) -> Option<&'a ImageRecord> {
    records.iter().find(|record| {
        record.template.is_some()
            && (record.image_id == image_id
                || record.image_id == requested
                || record.reference == requested)
    })
}

impl App {
    /// The template and values a new container from this image runs, or `None` without one.
    pub(crate) fn pin_template(
        &self,
        requested: &str,
        image_id: &str,
        given: &BTreeMap<String, String>,
    ) -> anyhow::Result<Option<(String, Pinned)>> {
        let records: Vec<ImageRecord> = self.db.list("image")?;
        let Some(record) = template_image(&records, requested, image_id) else {
            if let Some(name) = given.keys().next() {
                bail!("The image has no template, so it takes no parameter {name}");
            }
            return Ok(None);
        };
        let template = record.template.clone().context("Missing template")?;
        let values = template.resolve(given)?;
        Ok(Some((record.id.clone(), Pinned { template, values })))
    }

    /// The template a container was created with, which a snapshot of it keeps.
    pub(crate) fn pinned_template(&self, container_id: &str) -> Option<Template> {
        self.db
            .get::<Pinned>(PINNED, container_id)
            .ok()
            .map(|pinned| pinned.template)
    }

    pub(crate) fn save_pinned(&self, container_id: &str, pinned: &Pinned) -> anyhow::Result<()> {
        self.db.put(PINNED, container_id, pinned)
    }

    pub(crate) fn forget_pinned(&self, container_id: &str) -> anyhow::Result<()> {
        self.db.remove(PINNED, container_id)
    }

    /// Run the template setup of a container whose setup has not completed. A failure is
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
            .context("The template values of this container are missing; create it again")?;
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
            "Waiting for the desktop before running the template setup.",
        )?;
        let container = self.wait_ready(env).await?;
        let root = pinned.template.root;
        let user = if root { "root" } else { "1001:1001" };
        let names = pinned.values.keys().cloned().collect::<Vec<_>>();
        self.append_log(
            job_id,
            &format!(
                "Running the template setup as {user} (timeout: {} seconds). Parameters: {}.",
                pinned.template.timeout_seconds,
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
            .context("The template setup failed")?;
        self.append_log(job_id, "Template setup completed.")?;
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
                bail!("The container stopped before the template setup could run; check its logs");
            }
            if started.elapsed() > READY_LIMIT {
                bail!(
                    "The desktop was not ready within {} seconds, so the template setup did not run; check its logs",
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
                "The manager restarted before the template setup finished; retry to run it again"
                    .into()
            });
            db.put(CONTAINERS, &env.id, &env)?;
        }
    }
    Ok(())
}

pub async fn get(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Template>> {
    let record: ImageRecord = app.db.get("image", &id)?;
    record
        .template
        .map(Json)
        .ok_or_else(|| Error(StatusCode::NOT_FOUND, "This image has no template".into()))
}

pub async fn set(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(template): Json<Template>,
) -> Result<Json<Value>> {
    template.validate()?;
    replace(&app, &id, Some(template)).await
}

pub async fn remove(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    replace(&app, &id, None).await
}

async fn replace(app: &App, id: &str, template: Option<Template>) -> Result<Json<Value>> {
    let lock = app.resource_lock(&format!("image:{id}"));
    let _guard = lock.lock().await;
    let mut record: ImageRecord = app.db.get("image", id)?;
    record.template = template;
    app.db.put("image", id, &record)?;
    Ok(Json(crate::api::image_summary(&record)))
}

#[cfg(test)]
#[path = "templates_tests.rs"]
mod tests;
