//! The images published with Bilskirnir. They are listed with the saved images as kind
//! `catalog`, and an installation downloads each one the first time a container or a build
//! starts from it. The bundled list keeps the file name `desktop/profiles.json`, which the WSL
//! package's update command reads from the manager.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const DEFAULT_IMAGE: &str = "ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04";
/// The release this manager was built as. Only the release build sets it, through the
/// `BILSKIRNIR_RELEASE` build argument of the root Dockerfile.
const RELEASE: Option<&str> = option_env!("BILSKIRNIR_RELEASE");
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
    /// Where the image downloads from. A release downloads the desktop image published with it
    /// and tags it as `reference`, which containers, composes and archives record, so it keeps
    /// installing its own desktops however far the moving tag has gone since.
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
    serde_json::from_str(include_str!("../desktop/profiles.json"))
        .expect("Invalid bundled image catalog")
}
pub fn find(id: &str) -> Option<CatalogImage> {
    catalog().into_iter().find(|image| image.id == id)
}
pub fn from_image(image: &Value) -> Option<DesktopInfo> {
    let labels = &image["Config"]["Labels"];
    Some(DesktopInfo {
        catalog_id: labels["bilskirnir.profile"].as_str()?.to_owned(),
        title: labels["org.opencontainers.image.title"]
            .as_str()?
            .to_owned(),
        session: labels["bilskirnir.session"].as_str()?.to_owned(),
    })
}
/// A catalog image as the image list shows it, beside the saved images. `inspected` is Docker's
/// view of its reference, `None` when it is not downloaded; `docker_listed` is false when Docker
/// could not list images at all, which leaves availability unknown.
pub fn listed(image: &CatalogImage, inspected: Option<&Value>, docker_listed: bool) -> Value {
    let desktop = inspected.and_then(from_image);
    let usable = desktop
        .as_ref()
        .is_some_and(|d| d.catalog_id == image.id && d.session == image.session);
    let error = (inspected.is_some() && !usable).then_some("Image labels do not match the catalog");
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
