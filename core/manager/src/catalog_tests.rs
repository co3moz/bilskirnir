//! Pins down where a catalog image downloads from: its own catalog reference, unless the manager
//! was published, which takes the desktop image published under the same version or, for a build of
//! `master`, the same commit. And which desktop levels a manager takes: those from its oldest to
//! the one the desktop images it is built with carry. Also that the catalog is unique and labeled,
//! and that its images are listed with the saved ones whether they are downloaded or not.
use super::*;

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

fn image(reference: &str) -> CatalogImage {
    CatalogImage {
        id: "desktop".into(),
        name: "Desktop".into(),
        reference: reference.into(),
        session: "xfce".into(),
        build_target: "xfce".into(),
        description: String::new(),
        build_service: "desktop".into(),
    }
}

#[test]
fn a_release_downloads_every_catalog_image_under_its_own_version() {
    for image in catalog() {
        let (repository, _) = image.reference.rsplit_once(':').unwrap();
        assert_eq!(
            image.source_for(Some("1.2.3")),
            format!("{repository}:1.2.3")
        );
        assert_eq!(image.source_for(None), image.reference);
    }
}

#[test]
fn a_build_of_master_downloads_every_catalog_image_of_its_commit() {
    let build = format!("sha-{COMMIT}");
    for image in catalog() {
        let (repository, _) = image.reference.rsplit_once(':').unwrap();
        assert_eq!(
            image.source_for(Some(&build)),
            format!("{repository}:sha-{COMMIT}")
        );
    }
}

#[test]
fn both_desktop_images_carry_the_level_this_manager_handles() {
    let dockerfile = include_str!("../../../images/desktop/Dockerfile");
    let levels: Vec<&str> = dockerfile
        .lines()
        .filter_map(|line| line.trim().strip_prefix("LABEL thoric.desktop-level="))
        .collect();
    assert_eq!(levels, vec![DESKTOP_LEVEL.to_string(); 2]);
    // The manager takes the desktop images it is built with.
    assert_eq!(
        refusal_for(&json!(levels[0]), MIN_DESKTOP_LEVEL, DESKTOP_LEVEL),
        None
    );
}

#[test]
fn a_desktop_level_outside_what_the_manager_handles_is_refused() {
    // Every image from before the label is level 0.
    assert_eq!(refusal_for(&Value::Null, 0, 1), None);
    assert_eq!(refusal_for(&json!("1"), 0, 1), None);
    let newer = refusal_for(&json!("2"), 0, 1).unwrap();
    assert!(
        newer.contains("newer Thoric") && newer.contains("Update Thoric"),
        "{newer}"
    );
    let older = refusal_for(&Value::Null, 1, 2).unwrap();
    assert!(older.contains("older than this Thoric supports"), "{older}");
    for label in [json!(""), json!("one"), json!("-1"), json!("1.5"), json!(1)] {
        let refused = refusal_for(&label, 0, 1).unwrap();
        assert!(refused.contains("not a whole number"), "{label}: {refused}");
    }
    // A level too large to read is newer than any manager.
    assert!(
        refusal_for(&json!("99999999999999999999"), 0, 1)
            .unwrap()
            .contains("newer Thoric")
    );
    assert!(
        check_level(&json!({"thoric.desktop-level": (DESKTOP_LEVEL + 1).to_string()})).is_err()
    );
    assert!(check_level(&json!({"thoric.desktop": "true"})).is_ok());
}

#[test]
fn only_a_build_of_master_names_its_commit() {
    let build = format!("sha-{COMMIT}");
    assert_eq!(edge_commit_of(Some(&build)), Some(COMMIT));
    for release in [None, Some(""), Some("1.2.3"), Some("sha-")] {
        assert_eq!(edge_commit_of(release), None, "{release:?}");
    }
}

#[test]
fn a_reference_without_a_tag_downloads_as_it_is() {
    for reference in ["localhost:5000/desktop", "ghcr.io/x/desktop@sha256:abc"] {
        assert_eq!(image(reference).source_for(Some("1.2.3")), reference);
    }
    assert_eq!(
        image("localhost:5000/desktop:24.04").source_for(Some("1.2.3")),
        "localhost:5000/desktop:1.2.3"
    );
}

#[test]
fn image_catalog_is_unique_and_metadata_is_explicit() {
    let catalog = catalog();
    assert_eq!(catalog.len(), 2);
    assert_eq!(
        catalog
            .iter()
            .map(|p| &p.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        catalog.len()
    );
    assert!(
        catalog
            .iter()
            .any(|p| p.reference == DEFAULT_IMAGE && p.session == "xfce")
    );
    assert!(
        catalog
            .iter()
            .any(|p| p.id == "ubuntu-gnome" && p.session == "gnome")
    );
    assert!(
        from_image(&serde_json::json!({"Config":{"Labels":{"thoric.desktop":"true"}}})).is_none()
    );
    let info = from_image(&serde_json::json!({"Config":{"Labels":{"thoric.profile":"ubuntu-gnome","thoric.session":"gnome","org.opencontainers.image.title":"Ubuntu 24.04 · GNOME"}}})).unwrap();
    assert_eq!(info.session, "gnome");
    assert_eq!(info.catalog_id, "ubuntu-gnome");
    // Records saved while catalog images were profiles still carry their desktop.
    let old: DesktopInfo = serde_json::from_value(
        serde_json::json!({"profile":"ubuntu-xfce","title":"Ubuntu 24.04 · Lightweight XFCE","session":"xfce"}),
    )
    .unwrap();
    assert_eq!(old.catalog_id, "ubuntu-xfce");
}

#[test]
fn catalog_images_are_listed_with_the_saved_images() {
    let xfce = find("ubuntu-xfce").unwrap();
    let labels = |id: &str, session: &str| serde_json::json!({"Id":"sha256:x","Config":{"Labels":{"thoric.profile":id,"thoric.session":session,"org.opencontainers.image.title":"XFCE"}}});
    // Not downloaded yet: still listed, so a container can start from it and download it.
    let missing = listed(&xfce, None, true);
    assert_eq!(missing["kind"], "catalog");
    assert_eq!(missing["reference"], xfce.reference);
    assert!(missing["available"] == false && missing["downloaded"] == false);
    assert!(missing["image_id"].is_null() && missing["error"].is_null());
    let ready = listed(&xfce, Some(&labels("ubuntu-xfce", "xfce")), true);
    assert!(ready["available"] == true && ready["image_id"] == "sha256:x");
    assert_eq!(ready["desktop"]["catalog_id"], "ubuntu-xfce");
    // A tag that points at another desktop is reported instead of being used.
    let foreign = listed(&xfce, Some(&labels("ubuntu-gnome", "gnome")), true);
    assert!(foreign["available"] == false && foreign["downloaded"] == true);
    assert!(foreign["error"].is_string());
    // A catalog reference made for a newer Thoric, such as one pulled by hand, says why it waits.
    let mut newer = labels("ubuntu-xfce", "xfce");
    newer["Config"]["Labels"]["thoric.desktop-level"] =
        serde_json::json!((DESKTOP_LEVEL + 1).to_string());
    let newer = listed(&xfce, Some(&newer), true);
    assert!(newer["available"] == false && newer["downloaded"] == true);
    assert!(
        newer["error"].as_str().unwrap().contains("newer Thoric"),
        "{newer}"
    );
    // Without Docker nobody knows.
    assert!(listed(&xfce, None, false)["available"].is_null());
}
