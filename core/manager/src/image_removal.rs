//! Deleting an image. A saved image goes with its record: Docker removes the tag Bilskirnir gave
//! it, and the image itself once no other tag names it and no other image or container uses it.
//! A catalog image loses only its download and stays listed, to be downloaded again. An image
//! registered from the Docker host (kind `import`) was there before Bilskirnir, so only its record
//! goes. Never while a container record starts from it, unless another saved image's tag keeps
//! the same Docker image: a reset, a retry and a recreation create the container from its source
//! image again. Nothing is forced and nothing is pruned.
use crate::{
    App,
    catalog::CatalogImage,
    error::{Error, Result},
    model::{CONTAINERS, Container, ImageRecord},
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};
use std::sync::Arc;

/// `DELETE /api/images/{id}`, where `id` is a saved image's record id or a catalog id. It answers
/// at once, like deleting a volume: Docker removes an image in one call, and a refusal reaches the
/// caller as its status instead of a job log.
pub async fn remove(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    let lock = app.resource_lock(&format!("image:{id}"));
    // A download or a build of a catalog image holds its lock for minutes, and an agent's call
    // would time out behind it; the image is busy instead.
    let Ok(_guard) = lock.try_lock() else {
        return Err(Error(
            StatusCode::CONFLICT,
            "Another operation on this image is running; delete it once that finishes".into(),
        ));
    };
    let removed = match crate::catalog::find(&id) {
        Some(image) => app.delete_catalog_image(&image).await?,
        None => app.delete_saved_image(&id).await?,
    };
    Ok(Json(removed))
}

/// The container records that start from the Docker image `image_id`.
pub(crate) fn started_from<'a>(image_id: &str, containers: &'a [Container]) -> Vec<&'a Container> {
    containers
        .iter()
        .filter(|container| container.source_image_id == image_id)
        .collect()
}

impl App {
    /// Remove a catalog image's download. The caller holds the image's lock.
    async fn delete_catalog_image(&self, image: &CatalogImage) -> Result<Value> {
        if !self.image_present(&image.reference).await? {
            return Err(Error(
                StatusCode::NOT_FOUND,
                "This catalog image is not downloaded".into(),
            ));
        }
        let info = self.inspect("image", &image.reference).await?;
        let image_id = info["Id"].as_str().unwrap_or_default();
        self.refuse_used(image_id)?;
        let removed = self.remove_docker_image(&image.reference, image_id).await?;
        Ok(json!({"id": image.id, "name": image.name, "deleted": true, "image_removed": removed}))
    }

    /// Delete a saved image's record and, unless it was registered from the host, its Docker
    /// image. The caller holds the image's lock.
    async fn delete_saved_image(&self, id: &str) -> Result<Value> {
        let record: ImageRecord = self.db.get("image", id)?;
        let removed = if record.kind == "import" {
            false
        } else {
            // Another record's tag keeps the same Docker image, such as an archive imported on
            // the host it came from, so the containers keep their source.
            let records = self.db.list::<ImageRecord>("image")?;
            if !shares_image(&record, &records) {
                self.refuse_used(&record.image_id)?;
            }
            // Bilskirnir's own tag names the image. Once it is gone or names another image, the
            // image ID does, which Docker refuses while another tag still names it.
            let tagged = self
                .inspect("image", &record.reference)
                .await
                .is_ok_and(|info| info["Id"] == record.image_id.as_str());
            let target = if tagged {
                &record.reference
            } else {
                &record.image_id
            };
            self.remove_docker_image(target, &record.image_id).await?
        };
        self.db.remove("image", id)?;
        Ok(json!({"id": record.id, "name": record.name, "deleted": true, "image_removed": removed}))
    }

    /// Drop the tags of snapshots and imports that got no record, which a manager stopped
    /// between tagging the image and saving the record leaves behind. Run at startup, before
    /// any job runs, so a tag whose record is still to come is never seen. Only Bilskirnir's
    /// own repositories are looked at, and the image goes with its last tag, which is this
    /// orphan's own.
    pub(crate) async fn drop_orphan_tags(&self) -> anyhow::Result<Vec<String>> {
        let records = self.db.list::<ImageRecord>("image")?;
        let named: std::collections::HashSet<&str> = records
            .iter()
            .map(|record| record.reference.as_str())
            .collect();
        let mut dropped = Vec::new();
        for repository in ["bilskirnir/snapshot", "bilskirnir/imported"] {
            let listed = self
                .docker(&[
                    "image",
                    "ls",
                    "--format",
                    "{{.Repository}}:{{.Tag}}",
                    repository,
                ])
                .await?;
            for tag in String::from_utf8_lossy(&listed)
                .lines()
                .map(str::trim)
                .filter(|tag| tag.starts_with(repository) && !tag.ends_with(":<none>"))
            {
                if named.contains(tag) {
                    continue;
                }
                match self.docker(&["image", "rm", tag]).await {
                    Ok(_) => dropped.push(tag.to_owned()),
                    Err(error) => tracing::debug!("Orphan tag {tag}: {error:#}"),
                }
            }
        }
        Ok(dropped)
    }

    /// Refuse while a container record starts from the Docker image `image_id`.
    fn refuse_used(&self, image_id: &str) -> Result<()> {
        let containers = self.db.list::<Container>(CONTAINERS)?;
        let users = started_from(image_id, &containers);
        if users.is_empty() {
            Ok(())
        } else {
            Err(Error(StatusCode::CONFLICT, in_use(&users)))
        }
    }

    /// Remove `target`, a tag or an image ID, and tell whether Docker deleted the image `image_id`
    /// itself rather than only the tag. The containerd image store reports a `Deleted:` line for
    /// an image that a committed child, such as a snapshot, still names, and keeps it until the
    /// child goes; so the image is gone only once it no longer inspects either. An image that is
    /// gone already has nothing left to remove.
    async fn remove_docker_image(&self, target: &str, image_id: &str) -> Result<bool> {
        match self.docker(&["image", "rm", target]).await {
            Ok(output) => Ok(deleted_image(&String::from_utf8_lossy(&output))
                && self.inspect("image", image_id).await.is_err()),
            Err(error) if format!("{error:#}").contains("No such image") => Ok(false),
            Err(error) => Err(refused(error)),
        }
    }
}

/// Whether another saved image names the same Docker image as `record`.
fn shares_image(record: &ImageRecord, records: &[ImageRecord]) -> bool {
    records
        .iter()
        .any(|other| other.id != record.id && other.image_id == record.image_id)
}

/// Why an image stays while containers start from it, and what frees it.
fn in_use(users: &[&Container]) -> String {
    let names = users
        .iter()
        .map(|container| format!("\"{}\"", container.spec.name))
        .collect::<Vec<_>>()
        .join(", ");
    if users.len() == 1 {
        format!(
            "{names} was created from this image and needs it to reset; delete that container first"
        )
    } else {
        format!(
            "{names} were created from this image and need it to reset; delete those containers first"
        )
    }
}

/// Whether `docker image rm` reports deleting an image with a `Deleted:` line, rather than only
/// untagging it because another tag still names it or, on the classic store, a child needs it.
fn deleted_image(output: &str) -> bool {
    output.lines().any(|line| line.starts_with("Deleted:"))
}

/// Docker's refusal with a status: an image a container or another tag still holds is a conflict.
fn refused(error: anyhow::Error) -> Error {
    let message = format!("{error:#}");
    if message.contains("conflict") {
        Error(StatusCode::CONFLICT, message)
    } else {
        error.into()
    }
}

#[cfg(test)]
#[path = "image_removal_tests.rs"]
mod tests;
