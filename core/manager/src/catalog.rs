//! The images published with Thoric. They are listed with the saved images as kind
//! `catalog`, and an installation downloads each one the first time a container or a build
//! starts from it. The bundled list keeps the file name `desktop/profiles.json`, which the WSL
//! package's update command reads from the manager.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const DEFAULT_IMAGE: &str = "ghcr.io/co3moz/thoric/ubuntu-xfce:24.04";
/// What this manager was published as: its version for a release, `sha-<commit>` for a build of
/// `master`. Only the publishing workflow sets it, through the `THORIC_RELEASE` build argument of
/// the root Dockerfile, as the tag the desktop images of the same run carry.
const RELEASE: Option<&str> = option_env!("THORIC_RELEASE");

/// The commit of a published build of `master`, which carries the version of the release before
/// it. A release, or a build from the source, has none.
pub(crate) fn edge_commit() -> Option<&'static str> {
    edge_commit_of(RELEASE)
}
fn edge_commit_of(release: Option<&str>) -> Option<&str> {
    release?
        .strip_prefix("sha-")
        .filter(|commit| !commit.is_empty())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CatalogImage {
    pub id: String,
    pub name: String,
    pub reference: String,
    pub session: String,
    pub build_target: String,
    pub description: String,
    pub build_service: String,
}
impl CatalogImage {
    /// Where the image downloads from. A published manager downloads the desktop image published
    /// with it, under its version or its commit, and tags it as `reference`, which containers,
    /// composes and archives record, so it keeps installing its own desktops however far the
    /// moving tags have gone since.
    pub fn source(&self) -> String {
        self.source_for(RELEASE.filter(|release| !release.is_empty()))
    }
    fn source_for(&self, release: Option<&str>) -> String {
        match (release, self.reference.rsplit_once(':')) {
            (Some(release), Some((repository, tag)))
                if !tag.contains('/') && !repository.contains('@') =>
            {
                format!("{repository}:{release}")
            }
            _ => self.reference.clone(),
        }
    }
}
/// What an image's labels say about its desktop.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DesktopInfo {
    /// The catalog image the desktop derives from. Records saved before catalog images were
    /// listed with the others name it `profile`.
    #[serde(alias = "profile")]
    pub catalog_id: String,
    pub title: String,
    pub session: String,
}
pub fn catalog() -> Vec<CatalogImage> {
    serde_json::from_str(include_str!("../../../images/desktop/profiles.json"))
        .expect("Invalid bundled image catalog")
}
pub fn find(id: &str) -> Option<CatalogImage> {
    catalog().into_iter().find(|image| image.id == id)
}
/// The newest desktop level this manager handles. A desktop image names its level in the
/// `thoric.desktop-level` label, which images built, committed or imported from it keep, and one
/// without the label is level 0. The label in both final stages of `images/desktop/Dockerfile`
/// equals this, and the two are raised together when the desktop image changes in a way an older
/// manager cannot handle, so that manager refuses the image with a reason instead of failing
/// inside the desktop.
pub const DESKTOP_LEVEL: u32 = 1;
/// The oldest desktop level this manager still handles, raised when it stops coping with desktop
/// images from before a change.
pub const MIN_DESKTOP_LEVEL: u32 = 0;
const LEVEL_LABEL: &str = "thoric.desktop-level";

/// Refuses a desktop image, by its labels, whose level this manager does not handle.
pub fn check_level(labels: &Value) -> anyhow::Result<()> {
    match level_refusal(labels) {
        Some(reason) => anyhow::bail!(reason),
        None => Ok(()),
    }
}
/// Why this manager cannot use a desktop image with these labels, if it cannot.
pub fn level_refusal(labels: &Value) -> Option<String> {
    refusal_for(&labels[LEVEL_LABEL], MIN_DESKTOP_LEVEL, DESKTOP_LEVEL)
}
fn refusal_for(label: &Value, min: u32, max: u32) -> Option<String> {
    let level = match label {
        Value::Null => 0,
        Value::String(text) if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) => {
            text.parse().unwrap_or(u32::MAX)
        }
        _ => {
            return Some(format!(
                "The image's {LEVEL_LABEL} label is not a whole number"
            ));
        }
    };
    if level > max {
        Some(format!(
            "The image is made for a newer Thoric: its desktop level is {level}, and this Thoric \
             handles up to {max}. Update Thoric to use it."
        ))
    } else if level < min {
        Some(format!(
            "The image is older than this Thoric supports: its desktop level is {level}, and this \
             Thoric needs {min} or later. Download its catalog image again and build it again."
        ))
    } else {
        None
    }
}
pub fn from_image(image: &Value) -> Option<DesktopInfo> {
    let labels = &image["Config"]["Labels"];
    Some(DesktopInfo {
        catalog_id: labels["thoric.profile"].as_str()?.to_owned(),
        title: labels["org.opencontainers.image.title"]
            .as_str()?
            .to_owned(),
        session: labels["thoric.session"].as_str()?.to_owned(),
    })
}
/// A catalog image as the image list shows it, beside the saved images. `inspected` is Docker's
/// view of its reference, `None` when it is not downloaded; `docker_listed` is false when Docker
/// could not list images at all, which leaves availability unknown.
pub fn listed(image: &CatalogImage, inspected: Option<&Value>, docker_listed: bool) -> Value {
    let desktop = inspected.and_then(from_image);
    // A catalog reference made for a newer Thoric came from somewhere else, such as a pull by
    // hand; its update button downloads the one published with this manager again.
    let refusal = inspected.and_then(|info| level_refusal(&info["Config"]["Labels"]));
    let usable = refusal.is_none()
        && desktop
            .as_ref()
            .is_some_and(|d| d.catalog_id == image.id && d.session == image.session);
    let error = match (inspected, refusal) {
        (Some(_), Some(reason)) => Some(reason),
        (Some(_), None) if !usable => Some("Image labels do not match the catalog".to_owned()),
        _ => None,
    };
    json!({
        "id": image.id,
        "kind": "catalog",
        "name": image.name,
        "description": image.description,
        "reference": image.reference,
        "image_id": inspected.map(|info| &info["Id"]),
        "parent_image_id": null,
        "source_container": null,
        "created_at": null,
        "desktop": desktop,
        "available": if docker_listed { json!(usable) } else { Value::Null },
        "downloaded": inspected.is_some(),
        "error": error,
    })
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
