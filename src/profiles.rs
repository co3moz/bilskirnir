use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_IMAGE: &str = "ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04";
/// The release this manager was built as. Only the release build sets it, through the
/// `BILSKIRNIR_RELEASE` build argument of the root Dockerfile.
const RELEASE: Option<&str> = option_env!("BILSKIRNIR_RELEASE");
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub reference: String,
    pub session: String,
    pub build_target: String,
    pub description: String,
    pub build_service: String,
}
impl Profile {
    /// Where the profile downloads from. A release downloads the desktop image published with it
    /// and tags it as `reference`, which environments, templates and archives record, so it keeps
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
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DesktopInfo {
    pub profile: String,
    pub title: String,
    pub session: String,
}
pub fn catalog() -> Vec<Profile> {
    serde_json::from_str(include_str!("../desktop/profiles.json"))
        .expect("Invalid bundled profile catalog")
}
pub fn from_image(image: &Value) -> Option<DesktopInfo> {
    let labels = &image["Config"]["Labels"];
    Some(DesktopInfo {
        profile: labels["io.wsl-desktop.profile"].as_str()?.to_owned(),
        title: labels["org.opencontainers.image.title"]
            .as_str()?
            .to_owned(),
        session: labels["io.wsl-desktop.session"].as_str()?.to_owned(),
    })
}

#[cfg(test)]
#[path = "profiles_tests.rs"]
mod tests;
