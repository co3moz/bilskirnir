use crate::{
    App,
    error::{Error, Result, missing},
    jobs::launch,
    model::*,
};
use anyhow::Context;
use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

pub async fn session(State(app): State<Arc<App>>, headers: axum::http::HeaderMap) -> Json<Value> {
    Json(json!({"auth_enabled":app.config.auth,"authenticated":app.authenticated(&headers)}))
}
pub async fn settings(State(app): State<Arc<App>>) -> Json<Value> {
    Json(
        json!({"base_image":app.config.base_image,"audio_enabled":app.config.audio,"microphone_enabled":app.config.microphone,"wsl_gpu":app.config.wsl_gpu,"gpu_adapter":app.config.adapter}),
    )
}
pub async fn profiles(State(app): State<Arc<App>>) -> Json<Value> {
    let results = futures_util::future::join_all(crate::profiles::catalog().into_iter().map(|profile| {
        let app = app.clone();
        async move {
            match app.inspect("image", &profile.reference).await {
                Ok(info) => {
                    let desktop = crate::profiles::from_image(&info);
                    let available = desktop.as_ref().is_some_and(|d| d.profile == profile.id && d.session == profile.session);
                    json!({"profile": profile, "available": available, "image_id": info["Id"], "error": if available {None} else {Some("Image labels do not match the profile")}})
                }
                Err(_) => json!({"profile": profile, "available": false, "image_id": null}),
            }
        }
    })).await;
    Json(json!(results))
}
pub async fn build_profile(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let profile = crate::profiles::catalog()
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(missing)?;
    let resource = format!("profile:{}", profile.id);
    launch(
        app,
        "profile-build",
        resource,
        move |app, job_id| async move {
            app.append_log(
                &job_id,
                &format!(
                    "Building {}. Package installation requires internet access.",
                    profile.name
                ),
            )?;
            app.docker_logged(
                &job_id,
                &[
                    "build".into(),
                    "--progress=plain".into(),
                    "--target".into(),
                    profile.build_target.clone(),
                    "-t".into(),
                    profile.reference.clone(),
                    app.config.desktop_context.to_string_lossy().into_owned(),
                ],
            )
            .await?;
            let info = app.inspect("image", &profile.reference).await?;
            Ok(
                json!({"profile_id": profile.id, "reference": profile.reference, "image_id": info["Id"]}),
            )
        },
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
pub async fn environments(State(app): State<Arc<App>>) -> Result<Json<Value>> {
    let environments: Vec<Environment> = app.db.list("environment")?;
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
    let items = environments.into_iter().map(|e| {
        let docker = states.get(&e.container);
        let status = docker.and_then(|d| d["Status"].as_str()).unwrap_or_default();
        let health = if status.contains("(unhealthy)") { "unhealthy" } else if status.contains("(healthy)") { "healthy" } else if status.contains("(health: starting)") { "starting" } else { "unknown" };
        json!({"environment":e,"state":if ids.is_err() {"unavailable"} else {docker.and_then(|d|d["State"].as_str()).unwrap_or("missing")},"health":health,"status":docker.map(|d|&d["Status"])})
    }).collect::<Vec<_>>();
    Ok(Json(json!(items)))
}

pub async fn create(
    State(app): State<Arc<App>>,
    Json(spec): Json<EnvironmentSpec>,
) -> Result<Json<Value>> {
    spec.validate()?;
    let id = uuid::Uuid::new_v4().simple().to_string();
    launch(
        app,
        "create",
        format!("environment:{id}"),
        move |app, _| async move { app.create_environment(&id, spec).await },
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
    app.environment(&id)?;
    launch(
        app,
        &body.action.clone(),
        format!("environment:{id}"),
        move |app, _| async move {
            let env = app.environment(&id).map_err(|e| anyhow::anyhow!(e.1))?;
            let info = app.owned(&env).await?;
            app.docker(&[
                &body.action,
                info["Id"].as_str().context("Missing container ID")?,
            ])
            .await?;
            Ok(json!({"environment_id":id}))
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
    app.environment(&id)?;
    launch(
        app,
        "internet",
        format!("environment:{id}"),
        move |app, _| async move {
            let mut env = app.environment(&id).map_err(|e| anyhow::anyhow!(e.1))?;
            app.set_internet(&mut env, body.enabled).await?;
            Ok(json!({"environment_id":id,"internet":body.enabled}))
        },
    )
}
pub async fn remove(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    app.environment(&id)?;
    launch(
        app,
        "delete",
        format!("environment:{id}"),
        move |app, job_id| async move {
            let env = app.environment(&id).map_err(|e| anyhow::anyhow!(e.1))?;
            app.remove_environment(&env, &job_id).await?;
            Ok(
                json!({"deleted":id,"name":env.spec.name,"volumes_preserved":true,"images_preserved":true}),
            )
        },
    )
}
pub async fn logs(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    let env = app.environment(&id)?;
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
    app.environment(&id)?;
    launch(
        app,
        "retry",
        format!("environment:{id}"),
        move |app, _| async move { app.retry_environment(&id).await },
    )
}
pub async fn screenshot(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Response> {
    let _permit = app
        .screenshots
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let env = app.environment(&id)?;
    let info = app.owned(&env).await?;
    let bytes = app
        .docker(&[
            "exec",
            "--user",
            "desktop",
            "--env",
            "DISPLAY=:0",
            info["Id"].as_str().context("Missing container ID")?,
            "/usr/local/bin/desktop-screenshot",
        ])
        .await?;
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return Err(anyhow::anyhow!("Screenshot not ready").into());
    }
    Ok((
        [
            (header::CONTENT_TYPE, "image/jpeg"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        bytes,
    )
        .into_response())
}
pub async fn diagnostics(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let env = app.environment(&id)?;
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
pub async fn images(State(app): State<Arc<App>>) -> Result<Json<Value>> {
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
    Ok(Json(json!(
        records
            .into_iter()
            .map(|r| {
                let available = listed.as_ref().ok().map(|_| ids.contains(&r.image_id));
                let mut value = json!(r);
                value["available"] = json!(available);
                value
            })
            .collect::<Vec<_>>()
    )))
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
        return Err(anyhow::anyhow!("Image name must be 1–100 characters").into());
    }
    let _guard = app.operations.lock().await;
    let info = app.inspect("image", &body.reference).await?;
    if info["Config"]["Labels"]["io.wsl-desktop.desktop"] != "true" {
        return Err(anyhow::anyhow!("A desktop-compatible image is required").into());
    }
    let rec = ImageRecord {
        id: uuid::Uuid::new_v4().simple().to_string(),
        name: body.name,
        image_id: info["Id"].as_str().context("Missing image ID")?.into(),
        reference: body.reference,
        parent_image_id: None,
        source_environment: None,
        kind: "import".into(),
        created_at: now(),
        description: String::new(),
        desktop: crate::profiles::from_image(&info),
    };
    app.db.put("image", &rec.id, &rec)?;
    Ok(Json(json!(rec)))
}
#[derive(Deserialize)]
pub struct BuildImage {
    pub name: String,
    pub base_image: String,
    pub script: String,
    #[serde(default = "default_true")]
    pub internet: bool,
}
fn default_true() -> bool {
    true
}
pub async fn build(
    State(app): State<Arc<App>>,
    Json(body): Json<BuildImage>,
) -> Result<Json<Value>> {
    image_ref(&body.base_image)?;
    if body.script.len() > 1_000_000
        || body.name.trim().is_empty()
        || body.name.chars().count() > 100
    {
        return Err(anyhow::anyhow!("Invalid image name or script").into());
    }
    launch(
        app,
        "build",
        format!("build:{}", uuid::Uuid::new_v4()),
        move |app, id| async move {
            let base = app.inspect("image", &body.base_image).await?;
            if base["Config"]["Labels"]["io.wsl-desktop.desktop"] != "true" {
                anyhow::bail!("Base image must be desktop-compatible");
            }
            let parent = base["Id"]
                .as_str()
                .context("Missing base image ID")?
                .to_owned();
            let base_tag = format!("bilskirnir/build-base:{id}");
            app.docker(&["image", "tag", &parent, &base_tag]).await?;
            let dir = app.config.data.join("builds").join(&id);
            tokio::fs::create_dir_all(&dir).await?;
            tokio::fs::write(dir.join("setup.sh"), body.script.replace("\r\n", "\n")).await?;
            let dockerfile = format!(
                "FROM {base_tag}\nUSER root\nCOPY setup.sh /tmp/wsld-setup.sh\nRUN bash /tmp/wsld-setup.sh && rm /tmp/wsld-setup.sh\nLABEL io.wsl-desktop.desktop=true\nLABEL {LABEL}=true\n"
            );
            tokio::fs::write(dir.join("Dockerfile"), dockerfile).await?;
            let reference = format!("bilskirnir/custom:{id}");
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
            app.append_log(
                &id,
                "Running setup script as root. Mounted volumes are excluded from the image.",
            )?;
            app.docker_logged(&id, &args).await?;
            let info = app.inspect("image", &reference).await?;
            let rec = ImageRecord {
                id: id.clone(),
                name: body.name,
                image_id: info["Id"].as_str().context("Missing image ID")?.into(),
                reference,
                parent_image_id: Some(parent),
                source_environment: None,
                kind: "build".into(),
                created_at: now(),
                description: String::new(),
                desktop: crate::profiles::from_image(&info),
            };
            app.db.put("image", &id, &rec)?;
            Ok(json!(rec))
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
    app.environment(&env_id)?;
    if body.name.trim().is_empty()
        || body.name.chars().count() > 100
        || body.description.len() > 4096
    {
        return Err(anyhow::anyhow!("Invalid snapshot name or description").into());
    }
    launch(
        app,
        "snapshot",
        format!("environment:{env_id}"),
        move |app, id| async move {
            let env = app.environment(&env_id).map_err(|e| anyhow::anyhow!(e.1))?;
            let container = app.owned(&env).await?;
            let reference = format!("bilskirnir/snapshot:{id}");
            app.append_log(&id,"Committing container changes. Volumes, /run, /tmp and running application state are excluded.")?;
            app.docker_logged(
                &id,
                &[
                    "commit".into(),
                    "--change".into(),
                    format!("LABEL {LABEL}=true"),
                    container["Id"]
                        .as_str()
                        .context("Missing container ID")?
                        .into(),
                    reference.clone(),
                ],
            )
            .await?;
            let info = app.inspect("image", &reference).await?;
            let rec = ImageRecord {
                id: id.clone(),
                name: body.name,
                image_id: info["Id"].as_str().context("Missing image ID")?.into(),
                reference,
                parent_image_id: Some(env.source_image_id.clone()),
                source_environment: Some(env_id),
                kind: "snapshot".into(),
                created_at: now(),
                description: body.description,
                desktop: crate::profiles::from_image(&info),
            };
            app.db.put("image", &id, &rec)?;
            Ok(json!(rec))
        },
    )
}
pub async fn volumes(State(app): State<Arc<App>>) -> Result<Json<Value>> {
    let bytes = app
        .docker(&["volume", "ls", "--format", "{{json .}}"])
        .await?;
    let values = String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .collect::<Vec<_>>();
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
    let lock = app.resource_lock(&format!("volume:{}", body.name));
    let _guard = lock.lock().await;
    if app.optional_inspect("volume", &body.name).await?.is_some() {
        return Err(Error(
            StatusCode::CONFLICT,
            "A volume with this name already exists".into(),
        ));
    }
    app.docker(&[
        "volume",
        "create",
        "--label",
        &format!("{LABEL}=true"),
        &body.name,
    ])
    .await?;
    let created = app.inspect("volume", &body.name).await?;
    if created["Labels"][LABEL] != "true" {
        return Err(anyhow::anyhow!(
            "Volume was created by another operation; ownership does not match"
        )
        .into());
    }
    Ok(Json(json!({"name":body.name})))
}
pub async fn jobs(State(app): State<Arc<App>>) -> Result<Json<Value>> {
    let jobs = app.db.jobs_summary()?;
    Ok(Json(json!(jobs)))
}
pub async fn job(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    let job: Job = app.db.get("job", &id)?;
    Ok(Json(json!(job)))
}
