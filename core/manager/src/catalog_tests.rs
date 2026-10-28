//! Pins down where a catalog image downloads from: its own catalog reference, unless the
//! manager was published, which takes the desktop image published under the same version or,
//! for a build of `master`, the same commit.
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
