//! Pins down where a supplied profile downloads from: its own catalog reference, unless the
//! manager is a release, which takes the desktop image published under the same version.
use super::*;

fn profile(reference: &str) -> Profile {
    Profile {
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
fn a_release_downloads_every_supplied_profile_under_its_own_version() {
    for profile in catalog() {
        let (repository, _) = profile.reference.rsplit_once(':').unwrap();
        assert_eq!(
            profile.source_for(Some("1.2.3")),
            format!("{repository}:1.2.3")
        );
        assert_eq!(profile.source_for(None), profile.reference);
    }
}

#[test]
fn a_reference_without_a_tag_downloads_as_it_is() {
    for reference in ["localhost:5000/desktop", "ghcr.io/x/desktop@sha256:abc"] {
        assert_eq!(profile(reference).source_for(Some("1.2.3")), reference);
    }
    assert_eq!(
        profile("localhost:5000/desktop:24.04").source_for(Some("1.2.3")),
        "localhost:5000/desktop:1.2.3"
    );
}
