//! Following what the desktops declare: one helper per running desktop prints its declarations
//! whenever they change, and the manager reads the tools of a new or changed server once.
use super::{
    client,
    endpoints::announce,
    registry::{Found, SERVERS, ServerRecord, watch_args},
    servers::records,
};
use crate::{App, model::now};
use rmcp::{RoleClient, service::Peer};
use std::{process::Stdio, sync::Arc, time::Duration};
use tokio::io::{AsyncBufReadExt, BufReader};

/// How often the manager looks for desktops that started or stopped.
const SUPERVISE_EVERY: Duration = Duration::from_secs(10);

/// Watch every running desktop while MCP is enabled, and close idle server connections.
pub(crate) fn spawn(app: Arc<App>) {
    if !app.config.mcp_enabled {
        return;
    }
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(SUPERVISE_EVERY);
        loop {
            tick.tick().await;
            if let Err(error) = supervise(&app).await {
                tracing::debug!("Desktop MCP servers: {error:#}");
            }
            app.desktop_mcp.pool.reap();
        }
    });
}

/// Start a watcher for every running managed container that has none.
async fn supervise(app: &Arc<App>) -> anyhow::Result<()> {
    let output = app
        .docker(&[
            "ps",
            "--filter",
            &format!("label={}=true", crate::model::LABEL),
            "--format",
            "{{.ID}} {{.Label \"bilskirnir.environment\"}}",
        ])
        .await?;
    for line in String::from_utf8_lossy(&output).lines() {
        let Some((docker_id, record_id)) = line.split_once(' ') else {
            continue;
        };
        if record_id.is_empty() || app.container(record_id).is_err() {
            continue;
        }
        if app
            .desktop_mcp
            .watchers
            .lock()
            .unwrap()
            .insert(record_id.to_owned())
        {
            tokio::spawn(watcher(
                app.clone(),
                record_id.to_owned(),
                docker_id.to_owned(),
            ));
        }
    }
    Ok(())
}

/// Read a desktop's declarations as they change, until the desktop stops.
async fn watcher(app: Arc<App>, record_id: String, docker_id: String) {
    let child = app
        .command(&watch_args(&docker_id))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    if let Ok(mut child) = child {
        // stdin stays open: the helper ends when it closes, with this task.
        let _stdin = child.stdin.take();
        if let Some(stdout) = child.stdout.take() {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let found = serde_json::from_str(&line)
                    .map_err(anyhow::Error::from)
                    .and_then(|listing| super::registry::found(&listing));
                match found {
                    Ok(found) => apply(&app, &record_id, found).await,
                    Err(error) => tracing::debug!("Desktop MCP servers of {record_id}: {error:#}"),
                }
            }
        }
        let _ = child.kill().await;
    }
    app.desktop_mcp.watchers.lock().unwrap().remove(&record_id);
    // The tools stay listed while the container is stopped; its connections ended with it.
    app.desktop_mcp
        .pool
        .retain(|(_, container, _)| *container != record_id);
    announce(&app, &record_id);
}

/// Bring the records of a container in line with what its desktop declares. A new or changed
/// declaration has its tools read in the background.
pub(super) async fn apply(app: &Arc<App>, container_id: &str, found: Vec<Found>) {
    if app.container(container_id).is_err() {
        return;
    }
    let existing = records(app, container_id);
    let mut changed = false;
    let mut pending = Vec::new();
    for server in &found {
        let old = existing.iter().find(|record| record.name == server.name);
        let record = match &server.spec {
            Err(error) => ServerRecord {
                container_id: container_id.into(),
                name: server.name.clone(),
                source: server.source,
                spec: None,
                tools: Vec::new(),
                tools_at: None,
                error: Some(error.clone()),
            },
            Ok(spec) => {
                if let Some(old) = old
                    && old.spec.as_ref() == Some(spec)
                    && old.source == server.source
                    && old.error.is_none()
                {
                    if old.tools_at.is_none() {
                        pending.push(server.name.clone());
                    }
                    continue;
                }
                pending.push(server.name.clone());
                ServerRecord {
                    container_id: container_id.into(),
                    name: server.name.clone(),
                    source: server.source,
                    spec: Some(spec.clone()),
                    tools: Vec::new(),
                    tools_at: None,
                    error: None,
                }
            }
        };
        if let Some(old) = old
            && old.spec == record.spec
            && old.source == record.source
            && old.error == record.error
        {
            continue;
        }
        // A changed declaration ends the connections that run the old one.
        app.desktop_mcp
            .pool
            .retain(|(_, container, name)| !(container == container_id && *name == server.name));
        let _ = app.db.put(
            SERVERS,
            &ServerRecord::key(container_id, &server.name),
            &record,
        );
        changed = true;
    }
    for old in existing
        .iter()
        .filter(|old| !found.iter().any(|server| server.name == old.name))
    {
        let _ = app
            .db
            .remove(SERVERS, &ServerRecord::key(container_id, &old.name));
        app.desktop_mcp
            .pool
            .retain(|(_, container, name)| !(container == container_id && *name == old.name));
        changed = true;
    }
    if changed {
        announce(app, container_id);
    }
    for name in pending {
        let app = app.clone();
        let container_id = container_id.to_owned();
        tokio::spawn(async move { fetch(&app, &container_id, &name).await });
    }
}

/// Start a server once and keep the tools it lists, or why it could not list them. A server
/// already being read is left to that.
pub(super) async fn fetch(app: &Arc<App>, container_id: &str, name: &str) {
    let key = ServerRecord::key(container_id, name);
    if !app.desktop_mcp.fetching.lock().unwrap().insert(key.clone()) {
        return;
    }
    let result = async {
        let container = app
            .container(container_id)
            .map_err(|e| anyhow::anyhow!(e.1))?;
        let record: ServerRecord = app.db.get(SERVERS, &key)?;
        let tools = client::read_tools(app, &container, &record).await;
        anyhow::Ok((record, tools))
    }
    .await;
    app.desktop_mcp.fetching.lock().unwrap().remove(&key);
    let Ok((read, tools)) = result else {
        return;
    };
    // The declaration may have changed while it was read; that read is the one that counts.
    let Ok(mut record) = app.db.get::<ServerRecord>(SERVERS, &key) else {
        return;
    };
    if record.spec != read.spec {
        return;
    }
    match tools {
        Ok(tools) => {
            record.tools = tools;
            record.tools_at = Some(now());
            record.error = None;
        }
        Err(error) => {
            record.tools.clear();
            record.tools_at = None;
            record.error = Some(format!("{error:#}"));
        }
    }
    let _ = app.db.put(SERVERS, &key, &record);
    announce(app, container_id);
}

/// A server said its tools changed: read them through the connection that heard it.
pub(super) fn reread_tools(
    app: Arc<App>,
    container_id: &str,
    server: &str,
    peer: Peer<RoleClient>,
) {
    let key = ServerRecord::key(container_id, server);
    let container_id = container_id.to_owned();
    tokio::spawn(async move {
        let Ok(Ok(tools)) =
            tokio::time::timeout(Duration::from_secs(60), peer.list_all_tools()).await
        else {
            return;
        };
        let Ok(mut record) = app.db.get::<ServerRecord>(SERVERS, &key) else {
            return;
        };
        record.tools = tools;
        record.tools_at = Some(now());
        record.error = None;
        let _ = app.db.put(SERVERS, &key, &record);
        announce(&app, &container_id);
    });
}

impl super::DesktopMcp {
    /// Whether the declarations of a container are being watched, which they are while it runs.
    pub(super) fn watched(&self, container_id: &str) -> bool {
        self.watchers.lock().unwrap().contains(container_id)
    }
}
