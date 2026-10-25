//! Pins down what a restore refuses before it creates anything: a manifest or records that are
//! not a backup this release wrote, and any name this installation already uses, whether in its
//! records or in Docker. A volume that is merely missing is reported, not refused.
use super::*;
use crate::backup::{
    manifest::{Included, Installation, VolumeFacts, plan},
    tests::{container, endpoint, image, pinned},
};
use std::collections::BTreeMap;

fn backup() -> (Manifest, Records) {
    let installation = Installation {
        containers: vec![container(
            "c1",
            "Desk",
            "bilskirnir/custom:i1",
            &["projects", "absent"],
        )],
        images: vec![image("i1", "Tools", "build")],
        composes: vec![("c1".into(), pinned())],
        endpoints: vec![endpoint("desk", &["c1"])],
        available_images: HashSet::from(["i1".into()]),
        with_files: HashSet::from(["c1".into()]),
        volumes: BTreeMap::from([("projects".into(), VolumeFacts::default())]),
    };
    let included = Included {
        images: true,
        volumes: true,
        files: true,
        secrets: false,
    };
    plan(installation, included, "0.7.0", 1)
}

fn refused(manifest: &Manifest) -> String {
    format!("{:#}", check_manifest(manifest).unwrap_err())
}

#[test]
fn only_a_backup_this_release_reads_is_accepted() {
    let (manifest, records) = backup();
    check_manifest(&manifest).unwrap();
    check_records(&manifest, &records).unwrap();

    let mut other = manifest.clone();
    other.kind = "something-else".into();
    assert_eq!(refused(&other), "The file is not a Bilskirnir backup");
    let mut newer = manifest.clone();
    newer.format = FORMAT + 1;
    assert!(refused(&newer).contains("a layout this release does not read"));
    assert!(parse_manifest(b"{\"name\":\"an image archive\"}").is_err());
}

#[test]
fn names_that_reach_docker_must_be_ones_the_manager_gives() {
    let (manifest, _) = backup();
    for id in ["", "UPPER", "-dash", "a/b", "a b", &"x".repeat(65)] {
        let mut bad = manifest.clone();
        bad.containers[0].id = id.into();
        assert!(refused(&bad).contains("invalid container id"), "{id:?}");
    }
    let mut bad = manifest.clone();
    bad.volumes[0].name = "../etc".into();
    assert!(check_manifest(&bad).is_err());
    let mut bad = manifest.clone();
    bad.endpoints[0] = "Not An Endpoint".into();
    assert!(check_manifest(&bad).is_err());
    let mut bad = manifest.clone();
    bad.volumes[0].owner = Some("Other Owner".into());
    assert!(refused(&bad).contains("invalid owner labels"));
}

#[test]
fn the_parts_must_be_exactly_the_ones_the_manifest_says_it_carries() {
    let (manifest, _) = backup();
    let mut missing = manifest.clone();
    missing.parts.pop();
    assert!(refused(&missing).contains("parts do not match"));
    let mut twice = manifest.clone();
    twice.parts.push(twice.parts[0].clone());
    assert!(refused(&twice).contains("parts do not match"));
    let mut uncarried = manifest.clone();
    uncarried.images[0].image = false;
    assert!(refused(&uncarried).contains("parts do not match"));
}

#[test]
fn the_records_must_match_the_manifest_and_the_rules_of_their_routes() {
    let (manifest, records) = backup();
    let mut extra = records.clone();
    extra.containers.push(container("c9", "Stray", "x", &[]));
    assert!(
        check_records(&manifest, &extra)
            .unwrap_err()
            .to_string()
            .contains("containers do not match")
    );
    let mut stray = records.clone();
    stray.endpoints[0].grants[0].container_id = "elsewhere".into();
    assert!(
        check_records(&manifest, &stray)
            .unwrap_err()
            .to_string()
            .contains("names a container the backup does not hold")
    );
    let mut invalid = records.clone();
    invalid.containers[0].spec.resolution = "huge".into();
    assert!(check_records(&manifest, &invalid).is_err());
    let mut hash = records.clone();
    hash.endpoints[0].token_hash = Some("not-a-hash".into());
    assert!(check_records(&manifest, &hash).is_err());
    let mut orphan = records;
    orphan.composes[0].container_id = "elsewhere".into();
    assert!(check_records(&manifest, &orphan).is_err());
}

#[test]
fn a_restore_takes_only_names_nobody_uses() {
    let (manifest, _) = backup();
    let clear = review(&manifest, &Present::default());
    assert!(clear.conflicts.is_empty());
    assert_eq!(clear.missing_volumes, ["absent"]);

    let conflict = |present: Present| review(&manifest, &present).conflicts;
    let container = Conflict {
        kind: "container",
        name: "Desk".into(),
    };
    // Its record, its pinned compose, its Docker container or the one set aside, its networks and
    // the tag of its files all belong to the container.
    for present in [
        Present {
            containers: HashSet::from(["c1".into()]),
            ..Default::default()
        },
        Present {
            composes: HashSet::from(["c1".into()]),
            ..Default::default()
        },
        Present {
            docker_containers: HashSet::from(["bilskirnir-c1".into()]),
            ..Default::default()
        },
        Present {
            docker_containers: HashSet::from(["bilskirnir-c1-previous".into()]),
            ..Default::default()
        },
        Present {
            networks: HashSet::from(["bilskirnir-c1-egress".into()]),
            ..Default::default()
        },
        Present {
            tags: HashSet::from(["bilskirnir/container:restored-c1".into()]),
            ..Default::default()
        },
    ] {
        assert_eq!(conflict(present), std::slice::from_ref(&container));
    }
    let image = Conflict {
        kind: "image",
        name: "Tools".into(),
    };
    assert_eq!(
        conflict(Present {
            images: HashSet::from(["i1".into()]),
            ..Default::default()
        }),
        std::slice::from_ref(&image)
    );
    assert_eq!(
        conflict(Present {
            tags: HashSet::from(["bilskirnir/custom:i1".into()]),
            ..Default::default()
        }),
        [image]
    );
    assert_eq!(
        conflict(Present {
            endpoints: HashSet::from(["desk".into()]),
            ..Default::default()
        }),
        [Conflict {
            kind: "endpoint",
            name: "desk".into()
        }]
    );
    // A carried volume must be new here; one it only names may already be here.
    let present = Present {
        volumes: HashSet::from(["projects".into(), "absent".into()]),
        ..Default::default()
    };
    let reviewed = review(&manifest, &present);
    assert_eq!(
        reviewed.conflicts,
        [Conflict {
            kind: "volume",
            name: "projects".into()
        }]
    );
    assert!(reviewed.missing_volumes.is_empty());
    assert!(conflict_message(&reviewed.conflicts).contains("volume projects"));
}

#[test]
fn restored_images_go_by_bilskirnirs_own_tags() {
    assert_eq!(image_reference("build", "i1"), "bilskirnir/custom:i1");
    assert_eq!(image_reference("snapshot", "i1"), "bilskirnir/snapshot:i1");
    assert_eq!(image_reference("archive", "i1"), "bilskirnir/imported:i1");
    // One registered from the other host's own tag gets no name from that host.
    assert_eq!(image_reference("import", "i1"), "bilskirnir/imported:i1");
    assert_eq!(image_kind("import"), "archive");
    assert_eq!(image_kind("snapshot"), "snapshot");
    assert_eq!(files_reference("c1"), "bilskirnir/container:restored-c1");
    assert!(record_id("0123abcd-ef"));
}
