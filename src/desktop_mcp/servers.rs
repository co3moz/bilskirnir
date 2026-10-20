//! The dashboard's and the administrator's view of desktop servers: listing them, registering
//! one in a running desktop, removing it and reading its tools again.
use super::{
    registry::{Declaration, SERVERS, ServerRecord, Source, server_name},
    scopes::server_tool_names,
    watch,
};
use crate::{
    App,
    error::{Error, Result, missing},
    model::Container,
};
use anyhow::Context;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};
use std::sync::Arc;

/// A server as lists show it: never the values of its variables or headers. Each tool comes with
/// `exposed`, its name on an endpoint of this container alone, which an endpoint's deny list
/// uses.
pub(crate) fn summary(app: &App, record: &ServerRecord, container: &str, running: bool) -> Value {
    let mut value = match &record.spec {
        Some(spec) => spec.summary(),
        None => {
            json!({"description":"","transport":"stdio","command":null,"args":[],"cwd":null,"env":[],"url":null,"headers":[]})
        }
    };
    value["container_id"] = json!(record.container_id);
    value["container_name"] = json!(container);
    value["name"] = json!(record.name);
    value["source"] = json!(record.source);
    value["state"] = json!(record.state(running));
    value["error"] = json!(record.error);
    value["tools"] = json!(
        server_tool_names(record)
            .iter()
            .map(|named| json!({
                "name": named.tool.name,
                "description": named.tool.description.as_deref().unwrap_or_default(),
                "exposed": named.name,
            }))
            .collect::<Vec<_>>()
    );
    value["tools_at"] = json!(record.tools_at);
    value["log"] = json!(
        app.desktop_mcp
            .log_tail(&ServerRecord::key(&record.container_id, &record.name), 4096)
    );
    value
}

/// The servers the manager knows of a container, by name.
pub(super) fn records(app: &App, container_id: &str) -> Vec<ServerRecord> {
    let mut records: Vec<ServerRecord> = app
        .db
        .list::<ServerRecord>(SERVERS)
        .unwrap_or_default()
        .into_iter()
        .filter(|record| record.container_id == container_id)
        .collect();
    records.sort_by(|a, b| a.name.cmp(&b.name));
    records
}

/// Every desktop server the manager knows, as it last saw them.
pub async fn all_servers(State(app): State<Arc<App>>) -> Result<Json<Value>> {
    let mut items = Vec::new();
    let mut containers: Vec<Container> = app.db.list(crate::model::CONTAINERS)?;
    containers.sort_by(|a, b| a.spec.name.cmp(&b.spec.name));
    for container in containers {
        let running = app.desktop_mcp.watched(&container.id);
        for record in records(&app, &container.id) {
            items.push(summary(&app, &record, &container.spec.name, running));
        }
    }
    Ok(Json(json!({"items": items})))
}

/// A running container's Docker ID, or a conflict that says to start it.
async fn running(app: &App, container: &Container) -> Result<String> {
    let info = app.owned(container).await?;
    if info["State"]["Running"] != true {
        return Err(Error(
            StatusCode::CONFLICT,
            "Start the container before changing its MCP servers".into(),
        ));
    }
    Ok(info["Id"]
        .as_str()
        .context("Missing container ID")?
        .to_owned())
}

/// The servers of one container, read again from its desktop while it runs.
pub async fn container_servers(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
) -> Result<Json<Value>> {
    let container = app.container(&id)?;
    let info = app.owned(&container).await?;
    let running = info["State"]["Running"] == true;
    if running {
        let docker_id = info["Id"].as_str().context("Missing container ID")?;
        let found = app.read_servers(docker_id).await?;
        watch::apply(&app, &id, found).await;
    }
    let items = records(&app, &id)
        .iter()
        .map(|record| summary(&app, record, &container.spec.name, running))
        .collect::<Vec<_>>();
    Ok(Json(json!({"items": items})))
}

fn find(app: &App, id: &str, name: &str) -> Result<ServerRecord> {
    app.db
        .get(SERVERS, &ServerRecord::key(id, name))
        .map_err(|_| missing())
}

/// Register a server in the desktop user's folder, replacing one of the same name there, and
/// read its tools.
pub async fn register_server(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(declaration): Json<Declaration>,
) -> Result<Json<Value>> {
    declaration.validate()?;
    let container = app.container(&id)?;
    // Values are resolved now too, so a name that is not a compose value fails here.
    declaration
        .spec
        .resolve(&app.compose_values(&id))
        .map_err(|e| Error(StatusCode::BAD_REQUEST, format!("{e:#}")))?;
    let docker_id = running(&app, &container).await?;
    app.write_server(&docker_id, &declaration).await?;
    let found = app.read_servers(&docker_id).await?;
    watch::apply(&app, &id, found).await;
    watch::fetch(&app, &id, &declaration.name).await;
    let record = find(&app, &id, &declaration.name)?;
    Ok(Json(summary(&app, &record, &container.spec.name, true)))
}

/// Remove a server the desktop user registered. One the image declares stays: the image's files
/// are the desktop's, and an endpoint can leave it out instead.
pub async fn unregister_server(
    State(app): State<Arc<App>>,
    Path((id, name)): Path<(String, String)>,
) -> Result<Json<Value>> {
    server_name(&name)?;
    let container = app.container(&id)?;
    let record = find(&app, &id, &name)?;
    if record.source == Source::Image {
        return Err(Error(
            StatusCode::CONFLICT,
            format!(
                "The image declares {name}, so it cannot be removed; leave it out of an endpoint instead"
            ),
        ));
    }
    let docker_id = running(&app, &container).await?;
    if !app.remove_server(&docker_id, &name).await? {
        return Err(missing());
    }
    let found = app.read_servers(&docker_id).await?;
    watch::apply(&app, &id, found).await;
    Ok(Json(json!({"removed": true, "name": name})))
}

/// Start a server again and read its tools.
pub async fn refresh_server(
    State(app): State<Arc<App>>,
    Path((id, name)): Path<(String, String)>,
) -> Result<Json<Value>> {
    let container = app.container(&id)?;
    find(&app, &id, &name)?;
    running(&app, &container).await?;
    watch::fetch(&app, &id, &name).await;
    let record = find(&app, &id, &name)?;
    Ok(Json(summary(&app, &record, &container.spec.name, true)))
}
