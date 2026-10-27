//! What an endpoint's tool call reaches, for the agent activity log: the container a bound tool
//! works on, and a desktop server's tool by its own name with the annotations its server gave it.
use super::{
    registry::{SERVERS, ServerRecord},
    scopes::{Entry, Target},
};
use crate::{App, activity::Call};
use serde_json::Value;

/// Tell `call` what the endpoint's tool `entry`, called with `args`, reaches.
pub(super) fn describe(app: &App, entry: &Entry, args: &Value, call: &mut Call) {
    match &entry.target {
        Target::Thoric { containers, .. } => {
            // As the endpoint resolves it: its one container, or the one `container` names.
            let id = if containers.len() == 1 && args.get("container").is_none() {
                containers.first().map(|(_, id)| id.clone())
            } else {
                let alias = args["container"].as_str();
                containers
                    .iter()
                    .find(|(name, _)| Some(name.as_str()) == alias)
                    .map(|(_, id)| id.clone())
            };
            call.on_container(app, id);
        }
        // A job of any of the endpoint's containers.
        Target::Job { .. } => {}
        Target::Server {
            container_id,
            server,
            tool,
        } => {
            let annotations = app
                .db
                .get::<ServerRecord>(SERVERS, &ServerRecord::key(container_id, server))
                .ok()
                .and_then(|record| record.tools.into_iter().find(|t| &*t.name == tool.as_str()))
                .and_then(|tool| tool.annotations);
            call.on_server(server, tool, annotations.as_ref());
            call.on_container(app, Some(container_id.clone()));
        }
    }
}
