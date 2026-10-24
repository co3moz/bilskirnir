//! The dependency list of the Dependencies page: every interface package, crate, image, system
//! package, tool and workflow action Bilskirnir is built from. `scripts/dependencies.py` writes it
//! while the manager image builds, so it is always the list of this very build; a manager built
//! any other way has none, and says so.

use crate::error::{Error, Result};
use axum::{
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use std::path::Path;

/// Where the manager image keeps the list, beside Bilskirnir's license.
pub(crate) const LIST: &str = "/usr/share/doc/bilskirnir/dependencies.json";

/// `GET /api/dependencies`: the list as the build wrote it.
pub async fn list() -> Result<Response> {
    read(Path::new(LIST)).await
}

async fn read(path: &Path) -> Result<Response> {
    match tokio::fs::read(path).await {
        Ok(list) => Ok(([(header::CONTENT_TYPE, "application/json")], list).into_response()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(Error(
            StatusCode::NOT_FOUND,
            "This manager was built without a dependency list".into(),
        )),
        Err(error) => Err(Error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Could not read the dependency list: {error}"),
        )),
    }
}

#[cfg(test)]
#[path = "dependencies_tests.rs"]
mod tests;
