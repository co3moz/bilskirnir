//! Container groups: which names are allowed and which a member is reached by, that no two members
//! of a group answer to the same name, and that a group's network comes with its first member,
//! follows renames, and goes once no record lists the group.
use super::*;
use serde_json::json;

fn spec(name: &str, groups: &[&str], aliases: &[&str]) -> ContainerSpec {
    serde_json::from_value(json!({
        "name": name,
        "image": "example/desktop:1",
        "hostname": format!("thoric-{}", name.to_lowercase().replace(' ', "")),
        "groups": groups,
        "aliases": aliases,
    }))
    .unwrap()
}

fn record(id: &str, spec: ContainerSpec) -> Container {
    Container {
        id: id.into(),
        container: format!("thoric-{id}"),
        private_network: format!("thoric-{id}-private"),
        egress_network: format!("thoric-{id}-egress"),
        source_image_id: "sha256:desktop".into(),
        created_at: 1,
        spec,
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    }
}

#[test]
fn group_names_and_aliases_are_dns_labels() {
    let (longest, too_long) = ("x".repeat(63), "x".repeat(64));
    for good in ["web", "build-2", "a", longest.as_str()] {
        assert!(validate(&spec("A", &[good], &[good])).is_ok(), "{good}");
    }
    for bad in [
        "",
        "Web",
        "-web",
        "web-",
        "web.lan",
        "web_2",
        "ağ",
        too_long.as_str(),
    ] {
        assert!(validate(&spec("A", &[bad], &[])).is_err(), "{bad}");
        assert!(validate(&spec("A", &[], &[bad])).is_err(), "{bad}");
    }
    let error = validate(&spec("A", &["web", "web"], &[])).unwrap_err();
    assert_eq!(error.to_string(), "web is listed twice in the groups");
    let nine = ["a", "b", "c", "d", "e", "f", "g", "h", "i"];
    assert!(validate(&spec("A", &nine, &[])).is_err());
    assert!(validate(&spec("A", &[], &nine)).is_err());
}

#[test]
fn a_name_becomes_the_dns_label_the_dashboard_shows() {
    // The dashboard's groups.ts reads the same cases, so both say a container is reached alike.
    let cases: Vec<(String, Option<String>)> =
        serde_json::from_str(include_str!("../../web/tests/name-labels.json")).unwrap();
    assert!(cases.len() > 5);
    for (name, label) in cases {
        assert_eq!(name_label(&name), label, "{name}");
    }
}

#[test]
fn a_member_is_reached_by_its_hostname_name_and_aliases_once_each() {
    let mut web = spec("Web", &["build"], &["api", "web"]);
    web.hostname = Some("Thoric-Web".into());
    assert_eq!(names(&web), ["thoric-web", "web", "api"]);
}

#[test]
fn no_two_members_of_a_group_answer_to_the_same_name() {
    let others = [
        record("a", spec("Web", &["build"], &["api"])),
        record("b", spec("Db", &["data"], &["store"])),
    ];
    // A name of a member of a shared group is taken.
    let error = names_free("c", &spec("Api", &["build"], &[]), &others).unwrap_err();
    assert_eq!(
        error.to_string(),
        "\"Web\" in the group build is reached as api already; choose another name, hostname or alias"
    );
    // Outside a shared group the same name is free, and a container never clashes with itself.
    assert!(names_free("c", &spec("Api", &["data"], &[]), &others).is_ok());
    assert!(names_free("a", &spec("Web", &["build"], &["api"]), &others).is_ok());
    // Its proxy lets the names of the members of its groups through, and no others.
    let peers = peers("c", &spec("Tests", &["build"], &[]), &others);
    assert_eq!(peers, ["thoric-web", "web", "api"]);
}

#[test]
fn an_endpoint_with_the_wanted_names_is_kept() {
    let wanted = ["thoric-web".to_owned(), "web".to_owned()];
    let endpoint = json!({"Aliases": ["c0ffee000000", "web", "thoric-web"]});
    assert!(same_aliases(&endpoint, &wanted, "c0ffee000000ffff"));
    assert!(!same_aliases(&endpoint, &wanted[..1], "c0ffee000000ffff"));
    assert!(!same_aliases(&json!({"Aliases": null}), &wanted, "c0ffee"));
}

#[test]
fn the_list_says_whether_vnc_stays_in_the_private_network() {
    let ps = |labels: &str| json!({"Labels": labels});
    assert!(vnc_private(&ps(
        "org.opencontainers.image.description=A, B,thoric.vnc=private,thoric.managed=true"
    )));
    // An older image, and a container created without the private network's subnet.
    assert!(!vnc_private(&ps("thoric.desktop=true,thoric.managed=true")));
    assert!(!vnc_private(&ps("thoric.vnc=all,thoric.managed=true")));
}

/// A Docker CLI that keeps networks as files and logs every call.
#[cfg(unix)]
fn fake_docker() -> (std::sync::Arc<App>, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("thoric-groups-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    let docker = dir.join("docker");
    let script = r#"#!/bin/sh
cd 'DIR'
echo "$*" >> calls
case "$1 $2" in
  'network inspect') [ -f "net-$3" ] || exit 1; printf '[%s]' "$(cat "net-$3")" ;;
  'network ls') for file in net-*; do [ -f "$file" ] && echo "${file#net-}"; done; true ;;
  'network create')
    eval "name=\${$#}"
    printf '{"Id":"%s","Internal":true,"Driver":"bridge","Labels":{"thoric.managed":"true","thoric.group":"%s"}}' \
      "$name" "${name#thoric-group-}" > "net-$name"
    echo "$name" ;;
  'network rm') rm "net-$3" ;;
esac
"#;
    std::fs::write(&docker, script.replace("DIR", &dir.display().to_string())).unwrap();
    std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut app = crate::test_support::app(false);
    let config = &mut std::sync::Arc::get_mut(&mut app).unwrap().config;
    config.docker_bin = docker.display().to_string();
    config.data = dir.join("data");
    (app, dir)
}

#[cfg(unix)]
#[tokio::test]
async fn a_group_network_comes_with_its_first_member_and_goes_with_its_last() {
    let (app, dir) = fake_docker();
    let calls = || {
        let calls = std::fs::read_to_string(dir.join("calls")).unwrap_or_default();
        std::fs::write(dir.join("calls"), "").unwrap();
        calls
    };
    let inspect =
        |id: &str, networks: Value| json!({"Id": id, "NetworkSettings": {"Networks": networks}});
    let web = record("a", spec("Web", &["build"], &["api"]));
    let tests = record("b", spec("Tests", &["build"], &[]));
    app.db.put(CONTAINERS, "a", &web).unwrap();
    app.db.put(CONTAINERS, "b", &tests).unwrap();

    // The first member creates the internal network and joins it under its names.
    app.join_groups(&web, &inspect("c0ffee000000aaaa", json!({})))
        .await
        .unwrap();
    let made = calls();
    assert!(
        made.contains(
            "network create --internal --label thoric.managed=true --label thoric.group=build thoric-group-build\n"
        ),
        "{made}"
    );
    assert!(
        made.contains(
            "network connect --alias thoric-web --alias web --alias api thoric-group-build c0ffee000000aaaa\n"
        ),
        "{made}"
    );
    // The next one joins the same network.
    app.join_groups(&tests, &inspect("c0ffee000000bbbb", json!({})))
        .await
        .unwrap();
    let joined = calls();
    assert!(!joined.contains("network create"), "{joined}");
    assert!(joined.contains("network connect --alias thoric-tests --alias tests thoric-group-build c0ffee000000bbbb\n"), "{joined}");

    // Joined under the same names, nothing changes; a new name needs a new endpoint.
    let connected =
        json!({"thoric-group-build": {"Aliases": ["c0ffee000000", "thoric-web", "web", "api"]}});
    app.join_groups(&web, &inspect("c0ffee000000aaaa", connected.clone()))
        .await
        .unwrap();
    assert_eq!(calls(), "");
    let mut renamed = web.clone();
    renamed.spec.name = "Front".into();
    app.join_groups(&renamed, &inspect("c0ffee000000aaaa", connected.clone()))
        .await
        .unwrap();
    let moved = calls();
    let disconnect = moved
        .find("network disconnect thoric-group-build c0ffee000000aaaa\n")
        .unwrap();
    let connect = moved
        .find("network connect --alias thoric-web --alias front --alias api thoric-group-build c0ffee000000aaaa\n")
        .unwrap();
    assert!(disconnect < connect, "{moved}");

    // Leaving while another record lists the group keeps its network.
    let mut left = web.clone();
    left.spec.groups.clear();
    app.join_groups(&left, &inspect("c0ffee000000aaaa", connected))
        .await
        .unwrap();
    let kept = calls();
    assert!(
        kept.contains("network disconnect thoric-group-build c0ffee000000aaaa\n"),
        "{kept}"
    );
    assert!(!kept.contains("network rm"), "{kept}");
    app.db.put(CONTAINERS, "a", &left).unwrap();
    // The last member's deletion removes it.
    app.leave_groups(&tests).await.unwrap();
    assert!(calls().contains("network rm thoric-group-build\n"));
    assert!(!dir.join("net-thoric-group-build").exists());

    // A network of the group's name that Thoric did not make is refused.
    std::fs::write(
        dir.join("net-thoric-group-build"),
        r#"{"Id":"x","Internal":false,"Driver":"bridge","Labels":{}}"#,
    )
    .unwrap();
    let error = app
        .join_groups(&tests, &inspect("c0ffee000000bbbb", json!({})))
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "The network thoric-group-build exists, but Thoric did not create it for the group build"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
