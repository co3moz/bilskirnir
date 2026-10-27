//! A backup's records as this installation keeps them. Docker gives a loaded image the ID its own
//! image store computes, which need not be the one the backup's installation had, so every record
//! that names an image ID is moved to the image here; the restored images go by Thoric's own
//! tags; and the names of a container's Docker objects are made from its id again rather than
//! taken from the file.
use super::{
    manifest::Records,
    review::{docker_names, files_reference, image_kind, image_reference},
};
use crate::model::{Container, WorkingImage};
use std::collections::{HashMap, HashSet};

/// What the restore loaded and what Docker resolves here.
#[derive(Debug, Default)]
pub(crate) struct Loaded {
    /// The image ID each restored saved image got here, by record id.
    pub images: HashMap<String, String>,
    /// The image ID each container's files got here, by container id.
    pub files: HashMap<String, String>,
    /// The image ID here of each image ID or reference a container names that Docker resolves,
    /// such as a catalog image this installation has downloaded.
    pub resolved: HashMap<String, String>,
}

/// Move the records to what this installation has.
pub(crate) fn remap(mut records: Records, loaded: &Loaded) -> Records {
    // The backup's image IDs and tags of the images it restored, to their counterparts here.
    let mut ids = HashMap::new();
    let mut references = HashMap::new();
    for image in &records.images {
        if let Some(id) = loaded.images.get(&image.id) {
            ids.insert(image.image_id.clone(), id.clone());
            references.insert(
                image.reference.clone(),
                image_reference(&image.kind, &image.id),
            );
        }
    }
    for image in &mut records.images {
        if let Some(id) = loaded.images.get(&image.id) {
            image.image_id = id.clone();
        }
        image.reference = image_reference(&image.kind, &image.id);
        image.kind = image_kind(&image.kind).to_owned();
        if let Some(parent) = image.parent_image_id.as_ref().and_then(|p| ids.get(p)) {
            image.parent_image_id = Some(parent.clone());
        }
    }
    for container in &mut records.containers {
        let [name, _, private, egress] = docker_names(&container.id);
        container.container = name;
        container.private_network = private;
        container.egress_network = egress;
        if let Some(reference) = references.get(&container.spec.image) {
            container.spec.image = reference.clone();
        } else if let Some(id) = ids.get(&container.spec.image) {
            container.spec.image = id.clone();
        }
        // The same image here, the one the backup restored, or the one its settings name here.
        container.source_image_id = if loaded.resolved.contains_key(&container.source_image_id) {
            loaded.resolved[&container.source_image_id].clone()
        } else if let Some(id) = ids.get(&container.source_image_id) {
            id.clone()
        } else if let Some(id) = loaded.resolved.get(&container.spec.image) {
            id.clone()
        } else {
            container.source_image_id.clone()
        };
        container.working_image = loaded.files.get(&container.id).map(|id| WorkingImage {
            image_id: id.clone(),
            reference: files_reference(&container.id),
        });
        if container.working_image.is_none() {
            // Created from its image again, it lacks what its setup changed.
            crate::compose::setup_again(container);
        }
    }
    records
}

/// Why a restored container is not created now, which its record shows until a retry creates it;
/// `None` when it is created, stopped.
pub(crate) fn not_created(container: &Container, volumes: &HashSet<String>) -> Option<String> {
    if container.working_image.is_none() {
        return Some(
            "Restored without its files. Retry setup creates it from its image, downloading a catalog image when needed, and runs its compose setup again."
                .into(),
        );
    }
    container
        .mounts()
        .into_iter()
        .find(|mount| !volumes.contains(&mount.volume))
        .map(|mount| {
            format!(
                "Restored, but the volume {} is not on this installation. Restore or create it, then retry setup.",
                mount.volume
            )
        })
}

#[cfg(test)]
#[path = "remap_tests.rs"]
mod tests;
