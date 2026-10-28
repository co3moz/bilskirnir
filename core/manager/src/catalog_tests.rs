//! Pins down where a catalog image downloads from: its own catalog reference, unless the
//! manager was published, which takes the desktop image published under the same version or,
//! for a build of `master`, the same commit. And which desktop levels a manager takes: those
//! from its oldest to the one the desktop images it is built with carry.
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
