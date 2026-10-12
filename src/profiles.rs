use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_IMAGE: &str = "ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04";
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
