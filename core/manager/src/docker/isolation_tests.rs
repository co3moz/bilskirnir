//! Pins down when an internal network is made without an address on the Docker host: the
//! `docker network create` arguments, the engines that can, how the manager tells it runs in its
//! own container, and when an older private network is made again so, keeping its subnet, the
//! manager and the desktop's stopped container on it.
use super::*;
use serde_json::json;

#[test]
fn an_isolated_network_is_internal_in_gateway_mode_isolated() {
    let labels = [
        "thoric.managed=true".to_owned(),
        "thoric.environment=x".to_owned(),
    ];
    assert_eq!(
        create_args("thoric-x-private", &labels, Reach::Nothing, None).join(" "),
        "network create --internal --opt com.docker.network.bridge.gateway_mode_ipv4=isolated --label thoric.managed=true --label thoric.environment=x thoric-x-private"
    );
    // A network made again keeps the subnet its desktop was told.
    assert_eq!(
        create_args(
            "thoric-x-private",
            &labels,
            Reach::Nothing,
            Some("172.18.0.0/16")
        )
        .join(" "),
        "network create --internal --opt com.docker.network.bridge.gateway_mode_ipv4=isolated --label thoric.managed=true --label thoric.environment=x --subnet 172.18.0.0/16 thoric-x-private"
    );
    // An older engine refuses the option, and the internet network has a gateway on the host.
    assert_eq!(
        create_args("thoric-x-private", &labels, Reach::Host, None).join(" "),
        "network create --internal --label thoric.managed=true --label thoric.environment=x thoric-x-private"
    );
    assert_eq!(
        create_args("thoric-x-egress", &labels, Reach::Internet, None).join(" "),
        "network create --label thoric.managed=true --label thoric.environment=x thoric-x-egress"
    );
    let options = |options| json!({"Options": options});
    assert!(isolated(&options(
        json!({"com.docker.network.bridge.gateway_mode_ipv4": "isolated"})
    )));
    assert!(!isolated(&options(json!({}))));
    assert!(!isolated(&json!({})));
}

#[test]
fn docker_engine_28_is_the_first_to_isolate_and_29_to_keep_a_mac_address() {
    for (api, isolates, keeps_mac) in [
        ("1.47", false, false),
        ("1.48", true, false),
        ("1.49", true, false),
        ("1.53", true, false),
        ("1.54", true, true),
        ("1.56\n", true, true),
        ("2.0", true, true),
        // Not a version, or no answer from Docker.
        ("1.4", false, false),
        ("", false, false),
        ("one.48", false, false),
    ] {
        assert_eq!(api_at_least(api, ISOLATING_API), isolates, "{api:?}");
        assert_eq!(api_at_least(api, MAC_API), keeps_mac, "{api:?}");
    }
}

#[test]
fn the_manager_runs_in_the_container_whose_folder_its_hostname_comes_from() {
    let id = "b9e1d5814ac6676ab858cb7cae850bc4f4e7aa681dbac6d3d7b28c0942911e5f";
    // Docker 28 on ext4, and Docker 29 with its data on a mount of its own.
    let ext4 = format!(
        "829 820 8:32 /var/lib/docker/containers/{id}/resolv.conf /etc/resolv.conf rw,relatime - ext4 /dev/sdc rw\n\
         830 820 8:32 /var/lib/docker/containers/{id}/hostname /etc/hostname rw,relatime - ext4 /dev/sdc rw\n"
    );
    let own_mount = format!(
        "870 861 0:111 /containers/{id}/hostname /etc/hostname rw,nosuid - tmpfs tmpfs rw\n"
    );
    assert!(runs_in(&ext4, id));
    assert!(runs_in(&own_mount, id));
    // Another container, a process on the host, and a manager without an ID.
    let other = "a".repeat(64);
    assert!(!runs_in(&ext4, &other));
    assert!(!runs_in(&ext4, &id[..12]));
    let host = "22 1 8:32 / / rw,relatime shared:1 - ext4 /dev/sdc rw\n\
                98 22 0:47 / /var/lib/docker/rootfs/overlayfs/b9e1d5814ac6 rw - overlay overlay rw\n";
    assert!(!runs_in(host, id));
    assert!(!runs_in(&ext4, ""));
    // The container's hostname file mounted anywhere else is not its own.
    let elsewhere = format!(
        "830 820 8:32 /var/lib/docker/containers/{id}/hostname /mnt/hostname rw - ext4 /dev/sdc rw\n"
    );
    assert!(!runs_in(&elsewhere, id));
}

#[test]
fn a_desktop_listens_in_the_subnet_its_container_was_created_with() {
    let container = |env: serde_json::Value| json!({"Config": {"Env": env}});
    let told = container(json!([
        "PATH=/usr/bin",
        "DESKTOP_PRIVATE_SUBNET=172.18.0.0/16",
        "DESKTOP_SUDO=false",
    ]));
    assert_eq!(told_subnet(&told).as_deref(), Some("172.18.0.0/16"));
    // A container created before the manager told it, and values that are no IPv4 subnet.
    assert_eq!(told_subnet(&container(json!(["PATH=/usr/bin"]))), None);
    assert_eq!(told_subnet(&json!({})), None);
    for wrong in ["fd00::/64", "172.18.0.0", "172.18.0.0/33", "--subnet", ""] {
        let env = json!([format!("DESKTOP_PRIVATE_SUBNET={wrong}")]);
        assert_eq!(told_subnet(&container(env)), None, "{wrong}");
    }
}

/// A Docker CLI that answers what a private network's move asks and logs every call: the old
/// network until a new one is made, which fails while `refuse` exists, the engine's API version
/// from `api` and the containers on the network from `users`.
#[cfg(unix)]
fn fake_docker() -> (std::sync::Arc<App>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "thoric-isolation-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let docker = dir.join("docker");
    let script = r#"#!/bin/sh
cd 'DIR'
echo "$*" >> calls
case "$*" in
  'version --format {{.Server.APIVersion}}') cat api ;;
  'ps --all --no-trunc --filter network='*) cat users 2>/dev/null; true ;;
  'network create '*) [ -f refuse ] && exit 1; touch made; echo new ;;
  'network inspect new') echo '[{"Id":"new","Internal":true,"Options":{"com.docker.network.bridge.gateway_mode_ipv4":"isolated"},"Containers":{"manager":{}}}]' ;;
  'container inspect thoric-x') cat container ;;
  'container inspect thoric-manager') echo '[{"Id":"manager","Config":{"Labels":{"thoric.manager":"true"}}}]' ;;
  'network inspect thoric-x-private') cat private ;;
  'network '*) ;;
  *) exit 1 ;;
esac
"#;
    crate::test_support::write_script(&docker, &script.replace("DIR", &dir.display().to_string()));
    let mut app = crate::test_support::app(false);
    let config = &mut std::sync::Arc::get_mut(&mut app).unwrap().config;
    config.docker_bin = docker.display().to_string();
    config.manager = Some("thoric-manager".into());
    // A socket that is not there: Docker is reached through the CLI alone.
    config.docker_socket = Some(dir.join("docker.sock"));
    (app, dir)
}

#[cfg(unix)]
fn desktop() -> Container {
    let mut spec = crate::test_support::spec();
    spec.mac_address = Some("02:42:ac:11:00:99".into());
    Container {
        id: "x".into(),
        container: "thoric-x".into(),
        private_network: "thoric-x-private".into(),
        egress_network: "thoric-x-egress".into(),
        source_image_id: "sha256:desktop".into(),
        created_at: 1,
        spec,
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    }
}

#[cfg(unix)]
#[tokio::test]
async fn an_older_private_network_moves_only_while_nothing_that_runs_is_on_it() {
    let (app, dir) = fake_docker();
    let env = desktop();
    let old = json!({
        "Id": "old",
        "Internal": true,
        "IPAM": {"Config": [{"Subnet": "172.18.0.0/16", "Gateway": "172.18.0.1"}]},
        "Containers": {"manager": {}},
    });
    let state = |running: bool| {
        json!({
            "Id": "desk",
            "State": {"Running": running},
            "NetworkSettings": {"Networks": {"thoric-x-private": {}}},
        })
    };
    let (stopped, running) = (state(false), state(true));
    let users = |ids: &str| std::fs::write(dir.join("users"), ids).unwrap();
    let movable = async |container: Option<&Value>, api: &str| {
        app.movable(&env, &old, "manager", container, api)
            .await
            .unwrap()
    };

    users("manager\ndesk\n");
    // Docker 29 connects the stopped desktop again with its MAC address.
    assert!(movable(Some(&stopped), "1.56").await);
    // Its VNC server listens on the address the running session found.
    assert!(!movable(Some(&running), "1.56").await);
    // Docker 28 would give it an address of its own.
    assert!(!movable(Some(&stopped), "1.49").await);
    // Another container on the network, such as one a settings change set aside, keeps it.
    users("manager\ndesk\nprevious\n");
    assert!(!movable(Some(&stopped), "1.56").await);
    // A container yet to be created, after a reset, gets its address from `docker create`.
    users("manager\n");
    assert!(movable(None, "1.49").await);
    users("manager\nprevious\n");
    assert!(!movable(None, "1.56").await);

    std::fs::write(dir.join("calls"), "").unwrap();
    let manager = json!({"Id": "manager"});
    let made = app
        .isolate_again(&env, &old, &manager, Some(&stopped))
        .await
        .unwrap();
    assert!(isolated(&made));
    assert_eq!(
        std::fs::read_to_string(dir.join("calls")).unwrap(),
        "network disconnect old desk\n\
         network disconnect old manager\n\
         network rm old\n\
         network create --internal --opt com.docker.network.bridge.gateway_mode_ipv4=isolated --label thoric.managed=true --label thoric.environment=x --subnet 172.18.0.0/16 thoric-x-private\n\
         network inspect new\n\
         network connect new manager\n\
         network connect new desk\n\
         network inspect new\n"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_stopped_desktop_left_without_its_private_network_gets_it_before_it_starts() {
    let (app, dir) = fake_docker();
    let env = desktop();
    let container = |running: bool| {
        json!([{
            "Id": "desk",
            "Config": {"Labels": {"thoric.managed": "true", "thoric.environment": "x"}},
            "State": {"Running": running},
            "NetworkSettings": {"Networks": {}},
        }])
        .to_string()
    };
    // The tests do not run in the manager's container, so the network stays as it is.
    std::fs::write(
        dir.join("private"),
        json!([{
            "Id": "old",
            "Driver": "bridge",
            "Internal": true,
            "Labels": {"thoric.managed": "true", "thoric.environment": "x"},
            "Containers": {"manager": {}},
        }])
        .to_string(),
    )
    .unwrap();
    std::fs::write(dir.join("container"), container(true)).unwrap();
    let info = app.owned(&env).await.unwrap();
    std::fs::write(dir.join("calls"), "").unwrap();
    // A running desktop is started again as it is.
    app.before_start(&env, &info).await.unwrap();
    assert_eq!(std::fs::read_to_string(dir.join("calls")).unwrap(), "");

    std::fs::write(dir.join("container"), container(false)).unwrap();
    let info = app.owned(&env).await.unwrap();
    app.before_start(&env, &info).await.unwrap();
    let calls = std::fs::read_to_string(dir.join("calls")).unwrap();
    assert!(calls.ends_with("network connect old desk\n"), "{calls}");
    assert!(!calls.contains("network connect old manager"), "{calls}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_move_that_fails_leaves_the_next_start_the_subnet_the_desktop_was_told() {
    let (app, dir) = fake_docker();
    let env = desktop();
    let old = json!({
        "Id": "old",
        "Internal": true,
        "IPAM": {"Config": [{"Subnet": "172.18.0.0/16", "Gateway": "172.18.0.1"}]},
        "Containers": {"manager": {}},
    });
    let stopped = json!({
        "Id": "desk",
        "Config": {
            "Labels": {"thoric.managed": "true", "thoric.environment": "x"},
            "Env": ["DESKTOP_PRIVATE_SUBNET=172.18.0.0/16"],
        },
        "State": {"Running": false},
        "NetworkSettings": {"Networks": {"thoric-x-private": {}}},
    });
    // Another network took the subnet once the old one was gone: neither way of making it again
    // in that subnet works, and none is made in another.
    std::fs::write(dir.join("refuse"), "").unwrap();
    let manager = json!({"Id": "manager"});
    let error = app
        .isolate_again(&env, &old, &manager, Some(&stopped))
        .await
        .unwrap_err();
    assert!(
        format!("{error:#}")
            .contains("again in 172.18.0.0/16, the subnet its VNC server listens in"),
        "{error:#}"
    );
    let calls = std::fs::read_to_string(dir.join("calls")).unwrap();
    assert_eq!(calls.matches("network create").count(), 2, "{calls}");
    assert_eq!(
        calls.matches("--subnet 172.18.0.0/16").count(),
        2,
        "{calls}"
    );

    // The next start finds no private network and makes it in the subnet the container's
    // variable names, then connects the desktop again.
    std::fs::remove_file(dir.join("refuse")).unwrap();
    std::fs::write(dir.join("calls"), "").unwrap();
    let mut left = stopped.clone();
    left["NetworkSettings"]["Networks"] = json!({});
    std::fs::write(dir.join("container"), json!([left]).to_string()).unwrap();
    app.before_start(&env, &left).await.unwrap();
    let calls = std::fs::read_to_string(dir.join("calls")).unwrap();
    assert!(
        calls.contains(
            "network create --internal --label thoric.managed=true --label thoric.environment=x --subnet 172.18.0.0/16 thoric-x-private\n"
        ),
        "{calls}"
    );
    assert!(calls.ends_with("network connect new desk\n"), "{calls}");
    std::fs::remove_dir_all(&dir).unwrap();
}
