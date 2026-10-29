use crate::{
    App,
    error::{Error, Result, missing},
    jobs::{launch, launch_named, launch_retryable, retry},
    model::*,
    script_progress::{ScriptPart, ScriptProgress, TRACE, TRACE_NAME},
};
use anyhow::Context;
use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

pub async fn session(State(app): State<Arc<App>>, headers: axum::http::HeaderMap) -> Json<Value> {
    Json(json!({"auth_enabled":app.config.auth,"authenticated":app.authenticated(&headers)}))
}
pub async fn settings(State(app): State<Arc<App>>) -> Json<Value> {
    Json(
        json!({"version":env!("CARGO_PKG_VERSION"),"base_image":app.config.base_image,"audio_enabled":app.config.audio,"microphone_enabled":app.config.microphone,"gpu_enabled":app.config.gpu_enabled(),"wsl_gpu":app.config.wsl_gpu,"nvidia_compute":app.config.cuda,"dri_gpu":app.config.dri_gpu,"gpu_adapter":app.config.adapter,"gpu":crate::gpu::report(&app.config),"upload_limit_bytes":app.config.upload_limit,"archive_limit_bytes":app.config.archive_limit,"templates_url":app.config.templates_url,"volume_filter":app.config.volume_filter,"root_mcp_enabled":app.config.root_mcp_enabled,"internet_enabled":app.config.internet,"sudo_enabled":app.config.sudo,"gamepad_enabled":app.config.gamepad,"vpn_enabled":app.config.vpn,"proxy":app.config.proxy.as_ref().map(|proxy| json!({"url":proxy.url,"username":proxy.username,"bypass":proxy.bypass})),"defaults":app.config.feature_defaults()}),
    )
}
/// Build a catalog image from the bundled Docker context instead of downloading it.
pub async fn build_catalog(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let image = crate::catalog::find(&id).ok_or_else(missing)?;
    let resource = format!("image:{}", image.id);
    let request = retry::Request::SourceBuild {
        image_id: image.id.clone(),
    };
    launch_retryable(
        app,
        "source-build",
        resource,
        None,
        request,
        move |app, job_id| async move {
            app.append_log(
                &job_id,
                &format!(
                    "Building {}. Package installation requires internet access.",
                    image.name
                ),
            )?;
            app.docker_logged(
                &job_id,
                &[
                    "build".into(),
                    "--progress=plain".into(),
                    "--target".into(),
                    image.build_target.clone(),
                    "-t".into(),
                    image.reference.clone(),
                    app.config.desktop_context.to_string_lossy().into_owned(),
                ],
            )
            .await?;
            let info = app.inspect("image", &image.reference).await?;
            Ok(
                json!({"id": image.id, "kind": "catalog", "reference": image.reference, "image_id": info["Id"]}),
            )
        },
    )
}
/// Download a catalog image, or a newer release of it.
pub async fn pull_catalog(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let image = crate::catalog::find(&id).ok_or_else(missing)?;
    let resource = format!("image:{}", image.id);
    let request = retry::Request::ImagePull {
        image_id: image.id.clone(),
    };
    launch_retryable(
        app,
        "image-pull",
        resource,
        None,
        request,
        move |app, job_id| async move { app.pull_catalog_image(&image, &job_id).await },
    )
}
pub async fn system(State(app): State<Arc<App>>) -> Json<Value> {
    match app.docker(&["info", "--format", "{{json .}}"]).await {
        Ok(data) => {
            let info: Value = serde_json::from_slice(&data).unwrap_or_default();
            Json(
                json!({"connected":true,"version":info["ServerVersion"],"cpus":info["NCPU"],"memory_bytes":info["MemTotal"],"containers":info["Containers"],"manager_configured":app.config.manager.is_some()}),
            )
        }
        Err(e) => Json(json!({"connected":false,"error":format!("{e:#}")})),
    }
}
pub async fn containers(State(app): State<Arc<App>>) -> Result<Json<Value>> {
    let containers: Vec<Container> = app.db.list(CONTAINERS)?;
    let ids = app
        .docker(&[
            "ps",
            "-a",
            "--filter",
            &format!("label={LABEL}=true"),
            "--format",
            "{{json .}}",
        ])
        .await;
    let mut states = std::collections::HashMap::new();
    if let Ok(ref data) = ids {
        for line in String::from_utf8_lossy(data).lines() {
            if let Ok(value) = serde_json::from_str::<Value>(line)
                && let Some(name) = value["Names"].as_str()
            {
                states.insert(name.to_string(), value);
            }
        }
    }
    // Each record with Docker's view of it beside its own fields.
    let items = containers
        .into_iter()
        .map(|record| {
            let docker = states.get(&record.container);
            let status = docker
                .and_then(|d| d["Status"].as_str())
                .unwrap_or_default();
            let health = if status.contains("(unhealthy)") {
                "unhealthy"
            } else if status.contains("(healthy)") {
                "healthy"
            } else if status.contains("(health: starting)") {
                "starting"
            } else {
                "unknown"
            };
            let mut item = json!(record);
            item["state"] = json!(if ids.is_err() {
                "unavailable"
            } else {
                docker
                    .and_then(|d| d["State"].as_str())
                    .unwrap_or("missing")
            });
            item["health"] = json!(health);
            item["status"] = json!(docker.map(|d| &d["Status"]));
            // Whether the members of its groups are kept from its screen.
            item["vnc_private"] = json!(docker.map(crate::groups::vnc_private));
            item
        })
        .collect::<Vec<_>>();
    Ok(Json(json!(items)))
}

pub async fn container(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    app.container(&id)?;
    let Json(items) = containers(State(app)).await?;
    let item = items
        .as_array()
        .and_then(|items| items.iter().find(|item| item["id"] == id))
        .cloned()
        .ok_or_else(missing)?;
    Ok(Json(item))
}

/// A new container's settings, and values for the compose parameters of its image.
#[derive(Deserialize)]
pub struct CreateContainer {
    #[serde(flatten)]
    pub spec: ContainerSpec,
    #[serde(default)]
    pub parameters: std::collections::BTreeMap<String, String>,
    /// Give the container an MCP endpoint of its own once it is created.
    #[serde(default = "default_true")]
    pub mcp_endpoint: bool,
    /// The password of the container's own proxy, kept apart from its settings.
    #[serde(default)]
    pub proxy_password: Option<String>,
}
/// A create request with the installation's defaults for the features it leaves out. Only a
/// request gets them: the serde defaults of [`ContainerSpec`] fill the fields that records saved
/// before a field existed lack, and keep their meaning.
pub(crate) fn creation_request(
    config: &crate::config::Config,
    mut body: Value,
) -> Result<CreateContainer> {
    let defaults = config.feature_defaults();
    if let Value::Object(request) = &mut body {
        for (feature, default) in [
            ("gpu", defaults.gpu),
            ("internet", defaults.internet),
            ("sudo", defaults.sudo),
            ("audio", defaults.audio),
            ("gamepad", defaults.gamepad),
            ("vpn", defaults.vpn),
        ] {
            request.entry(feature).or_insert(json!(default));
        }
        // The microphone travels over the audio connection.
        let audio = request.get("audio") == Some(&Value::Bool(true));
        request.entry("microphone").or_insert(json!(
            audio && config.defaults.microphone && config.microphone
        ));
        // A container follows the installation's proxy unless it asks otherwise.
        request.entry("proxy").or_insert(if config.proxy.is_some() {
            json!({"mode": "installation"})
        } else {
            Value::Null
        });
    }
    serde_json::from_value(body).map_err(|e| {
        Error(
            StatusCode::BAD_REQUEST,
            format!("Invalid container request: {e}"),
        )
    })
}
pub async fn create(State(app): State<Arc<App>>, Json(body): Json<Value>) -> Result<Json<Value>> {
    let CreateContainer {
        mut spec,
        parameters,
        mcp_endpoint,
        proxy_password,
    } = creation_request(&app.config, body)?;
    let password =
        crate::outbound::PasswordChange::from_value(proxy_password.map(Value::String).as_ref())?;
    password.settle(None, &mut spec.proxy)?;
    spec.validate()?;
    let id = uuid::Uuid::new_v4().simple().to_string();
    // A compose found by the name the request uses is checked now; the job checks again once
    // Docker has resolved the image.
    let records: Vec<ImageRecord> = app.db.list("image")?;
    if let Some(record) = crate::compose::compose_image(&records, &spec.image, &spec.image)
        && let Some(compose) = &record.compose
    {
        let values = compose.resolve(&parameters, &id)?;
        crate::compose::check_mounts(&spec.mounts, &compose.mounts(&values))?;
    }
    let name = spec.name.clone();
    // The password is kept before the job, so the desktop's proxy has it as the desktop starts.
    app.save_proxy_password(&id, spec.proxy.as_ref(), &password)?;
    launch_named(
        app,
        "create",
        format!("container:{id}"),
        Some(name),
        move |app, job_id| async move {
            let created = app.create_container(&id, spec, parameters, &job_id).await;
            // A creation refused before its record was saved leaves no container to keep it for.
            if app.container(&id).is_err() {
                app.forget_proxy_password(&id)?;
            }
            let mut result = created?;
            if mcp_endpoint {
                let container = app.container(&id).map_err(|e| anyhow::anyhow!(e.1))?;
                // The container is made either way; an endpoint can still be created later.
                match crate::desktop_mcp::automatic_endpoint(&app, &container).await {
                    Ok(name) => {
                        app.append_log(
                            &job_id,
                            &format!("Created the MCP endpoint /mcp/{name} for this container."),
                        )?;
                        result["mcp_endpoint"] = json!(name);
                    }
                    Err(error) => app.append_log(
                        &job_id,
                        &format!("Could not create an MCP endpoint: {error:#}"),
                    )?,
                }
            }
            Ok(result)
        },
    )
}
#[derive(Deserialize)]
pub struct Action {
    pub action: String,
}
pub async fn action(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<Action>,
) -> Result<Json<Value>> {
    if !["start", "stop", "restart"].contains(&body.action.as_str()) {
        return Err(Error(StatusCode::BAD_REQUEST, "Invalid action".into()));
    }
    app.container(&id)?;
    launch(
        app,
        &body.action.clone(),
        format!("container:{id}"),
        move |app, _| async move {
            let env = app.container(&id).map_err(|e| anyhow::anyhow!(e.1))?;
            let info = app.owned(&env).await?;
            app.docker(&[
                &body.action,
                info["Id"].as_str().context("Missing container ID")?,
            ])
            .await
            .map_err(|error| app.device_error(&env, error))?;
            Ok(json!({"container_id":id}))
        },
    )
}
#[derive(Deserialize)]
pub struct Internet {
    pub enabled: bool,
}
pub async fn internet(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<Internet>,
) -> Result<Json<Value>> {
    app.container(&id)?;
    // Turning it off stays possible on a container that has it.
    if body.enabled && !app.config.internet {
        return Err(Error(
            StatusCode::BAD_REQUEST,
            "Internet access is disabled for this installation (INTERNET_ENABLED=false)".into(),
        ));
    }
    launch(
        app,
        "internet",
        format!("container:{id}"),
        move |app, _| async move {
            let mut env = app.container(&id).map_err(|e| anyhow::anyhow!(e.1))?;
            app.set_internet(&mut env, body.enabled).await?;
            Ok(json!({"container_id":id,"internet":body.enabled}))
        },
    )
}
/// Change a container's settings. `dry_run` reports which changes apply in place and which
/// restart the desktop, without a job.
pub async fn update_settings(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>> {
    let env = app.container(&id)?;
    let Value::Object(mut request) = body else {
        return Err(Error(
            StatusCode::BAD_REQUEST,
            "Settings must be a JSON object".into(),
        ));
    };
    let dry_run = match request.remove("dry_run") {
        None => false,
        Some(Value::Bool(dry_run)) => dry_run,
        Some(_) => {
            return Err(Error(
                StatusCode::BAD_REQUEST,
                "dry_run must be true or false".into(),
            ));
        }
    };
    let (_, changes) = app.settings_preview(&env, &request)?;
    if dry_run {
        return Ok(Json(
            json!({"container_id": id, "live": changes.live, "restart": changes.restart}),
        ));
    }
    launch(
        app,
        "settings",
        format!("container:{id}"),
        move |app, job_id| async move { app.update_container(&id, request, &job_id).await },
    )
}
/// How a container is deleted: its named volumes stay unless `delete_volumes` asks for those
/// created for it.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Remove {
    #[serde(default)]
    pub delete_volumes: bool,
}
pub async fn remove(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    axum::extract::Query(options): axum::extract::Query<Remove>,
) -> Result<Json<Value>> {
    app.container(&id)?;
    launch(
        app,
        "delete",
        format!("container:{id}"),
        move |app, job_id| async move {
            let env = app.container(&id).map_err(|e| anyhow::anyhow!(e.1))?;
            let removed = app
                .remove_container(&env, &job_id, options.delete_volumes)
                .await?;
            Ok(removal(
                &id,
                &env.spec.name,
                options.delete_volumes,
                removed,
            ))
        },
    )
}
/// A delete job's result. The volumes it went through are listed only when it was asked to.
pub(crate) fn removal(
    id: &str,
    name: &str,
    asked: bool,
    removed: crate::volumes::RemovedVolumes,
) -> Value {
    let mut result = json!({"deleted":id,"name":name,"volumes_preserved":removed.deleted.is_empty(),"images_preserved":true});
    if asked {
        result["volumes_deleted"] = json!(removed.deleted);
        result["volumes_kept"] = json!(removed.kept);
    }
    result
}
pub async fn logs(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    let env = app.container(&id)?;
    let info = app.owned(&env).await?;
    let args = [
        "logs",
        "--tail",
        "300",
        info["Id"].as_str().context("Missing container ID")?,
    ]
    .map(str::to_owned);
    let output = crate::process::capture(
        app.command(&args),
        Duration::from_secs(15),
        262_144,
        crate::process::Overflow::Tail,
    )
    .await?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(String::from_utf8_lossy(&output.stderr).into_owned()).into());
    }
    Ok(Json(
        json!({"log":format!("{}{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr))}),
    ))
}
pub async fn retry(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    app.container(&id)?;
    launch(
        app,
        "retry",
        format!("container:{id}"),
        move |app, job_id| async move { app.retry_container(&id, &job_id).await },
    )
}
pub async fn reset(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    app.container(&id)?;
    launch(
        app,
        "reset",
        format!("container:{id}"),
        move |app, job_id| async move { app.reset_container(&id, &job_id).await },
    )
}
pub async fn screenshot(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    axum::extract::Query(options): axum::extract::Query<crate::capture::Capture>,
) -> Result<Response> {
    let shot = crate::capture::capture(&app, &id, &options).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/jpeg".to_string()),
            (header::CACHE_CONTROL, "no-store".to_string()),
            (
                header::HeaderName::from_static("x-image-width"),
                shot.width.to_string(),
            ),
            (
                header::HeaderName::from_static("x-image-height"),
                shot.height.to_string(),
            ),
            (
                header::HeaderName::from_static("x-captured-at"),
                shot.captured_at.to_string(),
            ),
        ],
        shot.jpeg,
    )
        .into_response())
}
pub async fn diagnostics(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let env = app.container(&id)?;
    let info = app.owned(&env).await?;
    let bytes = app
        .docker(&[
            "exec",
            "--user",
            "desktop",
            info["Id"].as_str().context("Missing container ID")?,
            "/usr/local/bin/desktop-diagnostics",
        ])
        .await?;
    Ok(Json(
        json!({"output":String::from_utf8_lossy(&bytes),"gpu_requested":env.spec.gpu}),
    ))
}
/// Every image: the catalog first, then the saved images, each with its sizes.
pub async fn images(State(app): State<Arc<App>>) -> Result<Json<Value>> {
    let mut list = image_list(&app).await?;
    crate::layers::add_sizes(&app, &mut list).await;
    Ok(Json(json!(list)))
}
/// Every image as lists show it, before its sizes are added.
pub(crate) async fn image_list(app: &Arc<App>) -> Result<Vec<Value>> {
    let records = app.db.list::<ImageRecord>("image")?;
    let listed = app
        .docker(&["image", "ls", "-a", "--no-trunc", "--format", "{{.ID}}"])
        .await;
    let ids: std::collections::HashSet<String> = listed
        .as_ref()
        .map(|bytes| {
            String::from_utf8_lossy(bytes)
                .lines()
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let catalog =
        futures_util::future::join_all(crate::catalog::catalog().into_iter().map(|image| {
            let app = app.clone();
            let docker_listed = listed.is_ok();
            async move {
                let inspected = app.inspect("image", &image.reference).await.ok();
                crate::catalog::listed(&image, inspected.as_ref(), docker_listed)
            }
        }))
        .await;
    let saved = records.iter().map(|r| {
        let available = listed.as_ref().ok().map(|_| ids.contains(&r.image_id));
        let mut value = image_summary(r);
        value["available"] = json!(available);
        value["mcp_servers"] = json!(declared_servers(&records, r));
        if crate::compose::inherit::base_compose_missing(&records, r) {
            value["base_compose_missing"] = json!(true);
        }
        value
    });
    Ok(catalog
        .into_iter()
        .map(|mut value| {
            value["mcp_servers"] = json!([]);
            value
        })
        .chain(saved)
        .collect())
}
/// The desktop MCP servers an image's builds declared: its own build's and those of the images
/// it was built or snapshotted from, whose layers it carries.
pub(crate) fn declared_servers(records: &[ImageRecord], record: &ImageRecord) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut current = Some(record);
    for _ in 0..32 {
        let Some(image) = current else { break };
        for declaration in image.build.iter().flat_map(|build| &build.mcp) {
            if !names.contains(&declaration.name) {
                names.push(declaration.name.clone());
            }
        }
        current = image.parent_image_id.as_ref().and_then(|parent| {
            records
                .iter()
                .find(|other| other.image_id == *parent && other.id != image.id)
        });
    }
    names.sort();
    names
}
/// An image record as lists and job results show it. A build or setup script can be up to a
/// megabyte, so only the configuration and compose routes return them; an icon is named by its
/// version, and only the icon route returns the picture.
pub(crate) fn image_summary(record: &ImageRecord) -> Value {
    let mut value = json!(record);
    for (key, list) in [("build", "parts"), ("compose", "steps")] {
        if let Some(part) = value.get_mut(key).and_then(Value::as_object_mut) {
            part.remove("script");
            // The declarations may carry header values; lists name the servers instead.
            part.remove("mcp");
            for item in part
                .get_mut(list)
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
                .filter_map(Value::as_object_mut)
            {
                item.remove("script");
            }
        }
    }
    if let Some(icon) = &record.icon {
        value["icon"] = json!(icon.version());
    }
    value
}
/// The request that built an image, ready to build it again here or on another installation.
pub async fn image_config(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
) -> Result<Json<BuildImage>> {
    let record: ImageRecord = app.db.get("image", &id)?;
    let build = record.build.ok_or_else(|| {
        Error(
            StatusCode::NOT_FOUND,
            "No build configuration was recorded for this image".into(),
        )
    })?;
    // One template is named as builds named it before templates could be merged.
    let (template_id, template_ids) = match record.template_ids.as_slice() {
        [one] => (Some(one.clone()), Vec::new()),
        _ => (None, record.template_ids),
    };
    Ok(Json(BuildImage {
        name: record.name,
        description: record.description,
        base_image: build.base_image,
        internet: build.internet,
        script: build.script,
        parts: build.parts,
        mcp: build.mcp,
        compose: record.compose,
        inherit_compose: build.inherit_compose,
        icon: record.icon,
        template_id,
        template_ids,
    }))
}
#[derive(Deserialize)]
pub struct RegisterImage {
    pub reference: String,
    pub name: String,
}
pub async fn register_image(
    State(app): State<Arc<App>>,
    Json(body): Json<RegisterImage>,
) -> Result<Json<Value>> {
    image_ref(&body.reference)?;
    if body.name.trim().is_empty() || body.name.chars().count() > 100 {
        return Err(anyhow::anyhow!("Image name must be 1 to 100 characters").into());
    }
    let _guard = app.operations.lock().await;
    let info = app.inspect("image", &body.reference).await?;
    if info["Config"]["Labels"]["thoric.desktop"] != "true" {
        return Err(anyhow::anyhow!("A desktop-compatible image is required").into());
    }
    crate::catalog::check_level(&info["Config"]["Labels"])?;
    let rec = ImageRecord {
        id: uuid::Uuid::new_v4().simple().to_string(),
        name: body.name,
        image_id: info["Id"].as_str().context("Missing image ID")?.into(),
        reference: body.reference,
        parent_image_id: None,
        source_container: None,
        kind: "import".into(),
        created_at: now(),
        description: String::new(),
        desktop: crate::catalog::from_image(&info),
        build: None,
        compose: None,
        icon: None,
        template_ids: Vec::new(),
    };
    app.db.put("image", &rec.id, &rec)?;
    Ok(Json(json!(rec)))
}
/// A build request. `GET /api/images/{id}/config` returns the same shape, so an exported
/// configuration is a request that builds the image again, and a template on the templates page
/// is one too.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BuildImage {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub base_image: String,
    #[serde(default = "default_true")]
    pub internet: bool,
    /// The script of a build of one part, run as root on the base.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub script: String,
    /// Instead of a script, the scripts of templates merged into one build, in the order they
    /// run, each in a layer of its own.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<BuildPart>,
    /// The compose the built image gets. Configurations exported before templates were called
    /// composes name it `template`.
    #[serde(default, alias = "template", skip_serializing_if = "Option::is_none")]
    pub compose: Option<crate::compose::Compose>,
    /// Keep the compose of a saved base image before this one (`compose::inherit`).
    #[serde(
        default = "default_true",
        skip_serializing_if = "crate::model::is_true"
    )]
    pub inherit_compose: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<crate::icons::Icon>,
    /// Set by the templates page, so it can tell which of its templates are installed: one
    /// template here, several merged into one build in `template_ids`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub template_ids: Vec<String>,
    /// MCP servers the image declares for its desktops, written to `/etc/thoric/mcp.d`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp: Vec<crate::desktop_mcp::Declaration>,
}
impl BuildImage {
    fn validate(&self) -> anyhow::Result<()> {
        image_ref(&self.base_image)?;
        if self.name.trim().is_empty() || self.name.chars().count() > 100 {
            anyhow::bail!("Invalid image name or script");
        }
        build_scripts(&self.script, &self.parts)?;
        crate::desktop_mcp::check_declarations(&self.mcp)?;
        if self.description.len() > 4096 {
            anyhow::bail!("An image description must be at most 4096 bytes");
        }
        if let Some(compose) = &self.compose {
            compose.validate()?;
        }
        if let Some(icon) = &self.icon {
            icon.validate()?;
        }
        let templates = self.templates();
        if templates.len() > BUILD_PART_LIMIT {
            anyhow::bail!("An image is built from at most {BUILD_PART_LIMIT} templates");
        }
        for id in &templates {
            template_id(id)?;
        }
        Ok(())
    }

    /// The templates the image is built from, each once.
    pub(crate) fn templates(&self) -> Vec<String> {
        let mut ids = Vec::new();
        for id in self.template_id.iter().chain(&self.template_ids) {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
        ids
    }
}
fn default_true() -> bool {
    true
}
pub async fn build(
    State(app): State<Arc<App>>,
    Json(body): Json<BuildImage>,
) -> Result<Json<Value>> {
    body.validate()?;
    let name = body.name.clone();
    launch_retryable(
        app,
        "build",
        format!("build:{}", uuid::Uuid::new_v4()),
        Some(name),
        retry::Request::Build(body.clone()),
        move |app, id| async move {
            app.plan_stages(&id, &crate::stages::BUILD_STAGES)?;
            app.begin_stage(&id, "base")?;
            app.ensure_catalog_image(&body.base_image, &id).await?;
            let base = app.inspect("image", &body.base_image).await?;
            if base["Config"]["Labels"]["thoric.desktop"] != "true" {
                anyhow::bail!("Base image must be desktop-compatible");
            }
            crate::catalog::check_level(&base["Config"]["Labels"])?;
            let parent = base["Id"]
                .as_str()
                .context("Missing base image ID")?
                .to_owned();
            let (compose, template_ids) =
                crate::compose::inherit::built_on(&app, &id, &body, &parent)?;
            let base_tag = format!("thoric/build-base:{id}");
            app.docker(&["image", "tag", &parent, &base_tag]).await?;
            let dir = app.config.data.join("builds").join(&id);
            tokio::fs::create_dir_all(&dir).await?;
            let script = body.script.replace("\r\n", "\n");
            let parts = body
                .parts
                .iter()
                .map(|part| BuildPart {
                    script: part.script.replace("\r\n", "\n"),
                    ..part.clone()
                })
                .collect::<Vec<_>>();
            // Each part runs in a step of its own, named after it in the build's output, so a
            // failure says whose script it was and Docker's cache keeps the parts before it.
            let files = if parts.is_empty() {
                vec![ScriptPart {
                    id: "setup".into(),
                    name: body.name.clone(),
                    script: script.clone(),
                }]
            } else {
                parts
                    .iter()
                    .map(|part| ScriptPart {
                        id: part.id.clone(),
                        name: part.name.clone(),
                        script: part.script.clone(),
                    })
                    .collect()
            };
            // Bash reads the line trace first; a bind mount leaves nothing of it in the image.
            tokio::fs::write(dir.join(TRACE_NAME), TRACE).await?;
            let mut dockerfile = format!("FROM {base_tag}\nUSER root\n");
            for ScriptPart {
                id: name, script, ..
            } in &files
            {
                tokio::fs::write(dir.join(format!("{name}.sh")), script).await?;
                dockerfile.push_str(&format!(
                    "COPY {name}.sh /tmp/thoric-{name}.sh\nRUN --mount=type=bind,source={TRACE_NAME},target=/tmp/{TRACE_NAME} BASH_ENV=/tmp/{TRACE_NAME} bash /tmp/thoric-{name}.sh && rm /tmp/thoric-{name}.sh\n"
                ));
            }
            let mut progress = ScriptProgress::build(files);
            // After the scripts, so the declarations the build names are the ones that hold.
            if !body.mcp.is_empty() {
                dockerfile
                    .push_str(&crate::desktop_mcp::write_declarations(&dir, &body.mcp).await?);
            }
            dockerfile.push_str(&format!("LABEL thoric.desktop=true\nLABEL {LABEL}=true\n"));
            tokio::fs::write(dir.join("Dockerfile"), dockerfile).await?;
            let reference = format!("thoric/custom:{id}");
            let args = vec![
                "build".into(),
                "--progress=plain".into(),
                "--network".into(),
                if body.internet {
                    "default".into()
                } else {
                    "none".into()
                },
                "-t".into(),
                reference.clone(),
                dir.to_string_lossy().into_owned(),
            ];
            app.begin_stage(&id, "script")?;
            app.append_log(
                &id,
                &if parts.is_empty() {
                    "Running setup script as root. Mounted volumes are excluded from the image."
                        .into()
                } else {
                    format!(
                        "Running the scripts of {} as root, in this order, each in a layer of its own. Mounted volumes are excluded from the image.",
                        parts
                            .iter()
                            .map(|part| part.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                },
            )?;
            app.docker_build_logged(&id, &args, &mut progress).await?;
            // Without BuildKit's markers, the layer stage is skipped rather than guessed.
            app.begin_stage(&id, "image")?;
            let info = app.inspect("image", &reference).await?;
            let rec = ImageRecord {
                id: id.clone(),
                name: body.name,
                image_id: info["Id"].as_str().context("Missing image ID")?.into(),
                reference,
                parent_image_id: Some(parent),
                source_container: None,
                kind: "build".into(),
                created_at: now(),
                description: body.description,
                desktop: crate::catalog::from_image(&info),
                build: Some(BuildConfig {
                    base_image: body.base_image,
                    internet: body.internet,
                    script,
                    parts,
                    mcp: body.mcp,
                    inherit_compose: body.inherit_compose,
                }),
                compose,
                icon: body.icon,
                template_ids,
            };
            app.db.put("image", &id, &rec)?;
            Ok(image_summary(&rec))
        },
    )
}
#[derive(Deserialize)]
pub struct Snapshot {
    pub name: String,
    #[serde(default)]
    pub description: String,
}
pub async fn snapshot(
    State(app): State<Arc<App>>,
    Path(env_id): Path<String>,
    Json(body): Json<Snapshot>,
) -> Result<Json<Value>> {
    app.container(&env_id)?;
    if body.name.trim().is_empty()
        || body.name.chars().count() > 100
        || body.description.len() > 4096
    {
        return Err(anyhow::anyhow!("Invalid snapshot name or description").into());
    }
    let name = body.name.clone();
    let request = retry::Request::Snapshot {
        container_id: env_id.clone(),
        name: body.name.clone(),
        description: body.description.clone(),
    };
    launch_retryable(
        app,
        "snapshot",
        format!("container:{env_id}"),
        Some(name),
        request,
        move |app, id| async move {
            app.plan_stages(&id, &crate::stages::SNAPSHOT_STAGES)?;
            app.begin_stage(&id, "check")?;
            let env = app.container(&env_id).map_err(|e| anyhow::anyhow!(e.1))?;
            let container = app.owned(&env).await?;
            let reference = format!("thoric/snapshot:{id}");
            app.begin_stage(&id, "commit")?;
            app.append_log(&id,"Committing container changes. Volumes, /run, /tmp and running application state are excluded.")?;
            app.docker_logged(
                &id,
                &[
                    "commit".into(),
                    "--change".into(),
                    format!("LABEL {LABEL}=true"),
                    // The image's history says what this layer holds.
                    "--message".into(),
                    format!("Thoric snapshot of {}", env.spec.name),
                    container["Id"]
                        .as_str()
                        .context("Missing container ID")?
                        .into(),
                    reference.clone(),
                ],
            )
            .await?;
            app.begin_stage(&id, "record")?;
            let info = app.inspect("image", &reference).await?;
            // A container from the snapshot asks for the same parameters again.
            let compose = app.pinned_compose(&env_id);
            let records: Vec<ImageRecord> = app.db.list("image")?;
            let icon = crate::icons::inherited(&records, &env.source_image_id);
            let rec = ImageRecord {
                id: id.clone(),
                name: body.name,
                image_id: info["Id"].as_str().context("Missing image ID")?.into(),
                reference,
                parent_image_id: Some(env.source_image_id.clone()),
                source_container: Some(env_id),
                kind: "snapshot".into(),
                created_at: now(),
                description: body.description,
                desktop: crate::catalog::from_image(&info),
                build: None,
                compose,
                icon,
                template_ids: Vec::new(),
            };
            app.db.put("image", &id, &rec)?;
            Ok(image_summary(&rec))
        },
    )
}
pub async fn volumes(State(app): State<Arc<App>>) -> Result<Json<Value>> {
    let filter = app.config.volume_filter.as_deref();
    let bytes = app.docker_args(&crate::volumes::list_args(filter)).await?;
    let mut values = String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .collect::<Vec<_>>();
    let manager = app.listing_manager().await?;
    crate::volumes::mark_manager_volumes(&mut values, manager.as_ref());
    crate::volumes::mark_owners(&mut values, &app.db.list::<Container>(CONTAINERS)?);
    Ok(Json(json!(values)))
}
#[derive(Deserialize)]
pub struct Volume {
    pub name: String,
}
pub async fn create_volume(
    State(app): State<Arc<App>>,
    Json(body): Json<Volume>,
) -> Result<Json<Value>> {
    volume_name(&body.name)?;
    let _guard = app.hold(&format!("volume:{}", body.name)).await?;
    app.new_volume(&body.name).await?;
    Ok(Json(json!({"name":body.name})))
}
pub async fn jobs(State(app): State<Arc<App>>) -> Result<Json<Value>> {
    let jobs = app.db.jobs_summary()?;
    Ok(Json(json!(jobs)))
}
pub async fn job(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    let job: Job = app.db.job(&id)?;
    let mut value = json!(job);
    if retry::retryable(&app, &job) {
        value["retryable"] = json!(true);
    }
    Ok(Json(value))
}
#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;
