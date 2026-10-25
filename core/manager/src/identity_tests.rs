//! Pins down which hostnames, MAC addresses and machine IDs a container accepts and in which form
//! they are kept, that a new container gets an identity of its own while a record saved before
//! identities were kept takes its current container's, that every created container gets the
//! identity as flags, files and a network request, and the archive that carries the machine ID.
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn spec(extra: Value) -> ContainerSpec {
    let mut value =
        json!({"name": "Unity", "image": "ghcr.io/co3moz/bilskirnir/ubuntu-xfce:24.04"});
    value
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    serde_json::from_value(value).unwrap()
}

fn record(spec: ContainerSpec) -> Container {
    Container {
        id: "0123456789abcdef0123456789abcdef".into(),
        container: "bilskirnir-0123456789abcdef0123456789abcdef".into(),
        private_network: "bilskirnir-0123456789abcdef0123456789abcdef-private".into(),
        egress_network: "bilskirnir-0123456789abcdef0123456789abcdef-egress".into(),
        source_image_id: "sha256:source".into(),
        created_at: 0,
        spec,
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    }
}

#[test]
fn a_hostname_is_one_label_of_letters_digits_and_hyphens() {
    for valid in ["a", "unity-1", "DESKTOP-4F2A", &"x".repeat(63)] {
        assert!(hostname(valid).is_ok(), "{valid}");
    }
    for invalid in [
        "",
        "-unity",
        "unity-",
        "unity.lan",
        "unity_1",
        "unity 1",
        "ünity",
        &"x".repeat(64),
    ] {
        assert!(hostname(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn a_mac_address_is_one_unicast_interface_in_lowercase_with_colons() {
    for valid in [
        "02:42:ac:11:00:02",
        "00:1a:2b:3c:4d:5e",
        "fe:ff:ff:ff:ff:ff",
    ] {
        assert!(mac_address(valid).is_ok(), "{valid}");
    }
    for invalid in [
        "",
        "02:42:ac:11:00",
        "02:42:ac:11:00:02:03",
        "02:42:AC:11:00:02",
        "02-42-ac-11-00-02",
        "0242.ac11.0002",
        "02:42:ac:11:00:2",
        "02:42:ac:11:00:+2",
        "02:42:ac:11:00:0g",
        // The lowest bit of the first octet marks a group address.
        "01:00:5e:00:00:01",
        "ff:ff:ff:ff:ff:ff",
        "00:00:00:00:00:00",
    ] {
        assert!(mac_address(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn a_machine_id_is_32_lowercase_hex_digits_and_not_the_null_id() {
    assert!(machine_id("8aed3e38ba435c50deadb0896ace189b").is_ok());
    for invalid in [
        "",
        "8aed3e38ba435c50deadb0896ace189",
        "8aed3e38ba435c50deadb0896ace189bb",
        "8AED3E38BA435C50DEADB0896ACE189B",
        "8aed3e38-ba43-5c50-dead-b0896ace189b",
        "8aed3e38ba435c50deadb0896ace189g",
        "00000000000000000000000000000000",
    ] {
        assert!(machine_id(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn requests_may_write_an_identity_the_way_other_tools_print_it() {
    let spec = spec(json!({
        "hostname": "DESKTOP-4F2A",
        "mac_address": "02-42-AC-11-00-02",
        "machine_id": "8AED3E38-BA43-5C50-DEAD-B0896ACE189B"
    }));
    // A hostname keeps its case, which software may compare.
    assert_eq!(spec.hostname.as_deref(), Some("DESKTOP-4F2A"));
    assert_eq!(spec.mac_address.as_deref(), Some("02:42:ac:11:00:02"));
    assert_eq!(
        spec.machine_id.as_deref(),
        Some("8aed3e38ba435c50deadb0896ace189b")
    );
    spec.validate().unwrap();
    for (key, value) in [
        ("hostname", "unity.lan"),
        ("mac_address", "01:00:5e:00:00:01"),
        ("machine_id", "8aed3e38"),
    ] {
        assert!(spec_with(key, value).validate().is_err(), "{key}={value}");
    }
}

fn spec_with(key: &str, value: &str) -> ContainerSpec {
    spec(json!({ key: value }))
}

#[test]
fn generated_identities_are_valid_random_and_never_a_vendors_address() {
    let macs = (0..64).map(|_| generate_mac()).collect::<Vec<_>>();
    for mac in &macs {
        mac_address(mac).unwrap();
        let first = u8::from_str_radix(&mac[..2], 16).unwrap();
        assert_eq!(first & 0b11, 0b10, "{mac} is not locally administered");
    }
    let ids = (0..64).map(|_| generate_machine_id()).collect::<Vec<_>>();
    for id in &ids {
        machine_id(id).unwrap();
    }
    let distinct = |values: &[String]| {
        values
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
    };
    assert_eq!(distinct(&macs), 64);
    assert_eq!(distinct(&ids), 64);
}

#[test]
fn a_new_container_gets_what_its_request_left_out() {
    let mut chosen = spec(json!({"hostname": "unity-build", "mac_address": "02:00:00:00:00:01"}));
    assign(&mut chosen, "bilskirnir-new");
    assert_eq!(chosen.hostname.as_deref(), Some("unity-build"));
    assert_eq!(chosen.mac_address.as_deref(), Some("02:00:00:00:00:01"));
    machine_id(chosen.machine_id.as_deref().unwrap()).unwrap();
    // Two containers from the same request, such as two from one snapshot, differ.
    let (mut first, mut second) = (spec(json!({})), spec(json!({})));
    assign(&mut first, "bilskirnir-first");
    assign(&mut second, "bilskirnir-second");
    assert_eq!(first.hostname.as_deref(), Some("bilskirnir-first"));
    assert_ne!(first.mac_address, second.mac_address);
    assert_ne!(first.machine_id, second.machine_id);
    first.validate().unwrap();
}

#[test]
fn a_record_from_before_identities_takes_its_current_containers() {
    let mut env = record(spec(json!({})));
    let info = json!({
        "Config": {
            "Hostname": "4f02a86c82ec",
            "Labels": {LABEL: "true", "bilskirnir.environment": env.id}
        },
        "NetworkSettings": {"Networks": {
            "bilskirnir-0123456789abcdef0123456789abcdef-egress": {"MacAddress": "0a:00:00:00:00:09"},
            "bilskirnir-0123456789abcdef0123456789abcdef-private": {"MacAddress": "c6:1d:2a:3b:4c:5d"}
        }}
    });
    adopt(&mut env, Some(&info));
    assert_eq!(env.spec.hostname.as_deref(), Some("4f02a86c82ec"));
    // The private network's address: the one the container was created with.
    assert_eq!(env.spec.mac_address.as_deref(), Some("c6:1d:2a:3b:4c:5d"));
    // Its machine ID is still the one its image carries.
    assert_eq!(env.spec.machine_id, None);
    // Once kept, the identity is the record's, whatever the next container reports.
    let mut renamed = info.clone();
    renamed["Config"]["Hostname"] = json!("other");
    adopt(&mut env, Some(&renamed));
    assert_eq!(env.spec.hostname.as_deref(), Some("4f02a86c82ec"));
}

#[test]
fn without_its_own_container_a_record_gets_a_new_identity() {
    let foreign = json!({
        "Config": {
            "Hostname": "foreign",
            "Labels": {LABEL: "true", "bilskirnir.environment": "another"}
        },
        "NetworkSettings": {"Networks": {
            "bilskirnir-0123456789abcdef0123456789abcdef-private": {"MacAddress": "02:00:00:00:00:66"}
        }}
    });
    // A stopped container reports no address, and Docker gives it a new one at its next start.
    let stopped = json!({
        "Config": {
            "Hostname": "4f02a86c82ec",
            "Labels": {LABEL: "true", "bilskirnir.environment": "0123456789abcdef0123456789abcdef"}
        },
        "NetworkSettings": {"Networks": {
            "bilskirnir-0123456789abcdef0123456789abcdef-private": {"MacAddress": ""}
        }}
    });
    for info in [None, Some(&foreign)] {
        let mut env = record(spec(json!({})));
        adopt(&mut env, info);
        assert_eq!(env.spec.hostname.as_deref(), Some(env.container.as_str()));
        mac_address(env.spec.mac_address.as_deref().unwrap()).unwrap();
        assert_ne!(env.spec.mac_address.as_deref(), Some("02:00:00:00:00:66"));
        assert_eq!(env.spec.machine_id, None);
    }
    let mut env = record(spec(json!({})));
    adopt(&mut env, Some(&stopped));
    assert_eq!(env.spec.hostname.as_deref(), Some("4f02a86c82ec"));
    mac_address(env.spec.mac_address.as_deref().unwrap()).unwrap();
}

#[test]
fn every_created_container_gets_the_hostname_and_mac_address() {
    let app = crate::tests::app(false);
    let mut identified =
        spec(json!({"hostname": "unity-build", "mac_address": "02:00:00:00:00:01"}));
    identified.machine_id = Some(generate_machine_id());
    let args = app.container_args(&record(identified)).join(" ");
    assert!(args.contains("--hostname unity-build"), "{args}");
    assert!(args.contains("--mac-address 02:00:00:00:00:01"), "{args}");
    assert!(args.contains("--network bilskirnir-0123456789abcdef0123456789abcdef-private"));
    // The internet network is connected after creation, with the same address.
    assert!(!args.contains("-egress"), "{args}");
    // A record saved before identities were kept has none to give.
    let args = app.container_args(&record(spec(json!({}))));
    assert!(
        !args
            .iter()
            .any(|arg| arg == "--hostname" || arg == "--mac-address"),
        "{args:?}"
    );
}

/// The entries of an archive: name, mode, owner, group and contents.
fn entries(archive: &[u8]) -> Vec<(String, u32, u32, u32, Vec<u8>)> {
    let field = |block: &[u8], range: std::ops::Range<usize>| {
        let text = std::str::from_utf8(&block[range]).unwrap();
        u32::from_str_radix(text.trim_end_matches(['\0', ' ']), 8).unwrap()
    };
    let mut entries = Vec::new();
    let mut at = 0;
    while archive[at..at + 512].iter().any(|byte| *byte != 0) {
        let block = &archive[at..at + 512];
        let sum: u32 = block
            .iter()
            .enumerate()
            .map(|(i, byte)| u32::from(if (148..156).contains(&i) { b' ' } else { *byte }))
            .sum();
        assert_eq!(field(block, 148..156), sum, "checksum");
        assert_eq!(&block[257..265], b"ustar\x0000");
        assert_eq!(block[156], b'0', "a regular file");
        let name = String::from_utf8(block[..100].split(|b| *b == 0).next().unwrap().to_vec());
        let size = field(block, 124..136) as usize;
        entries.push((
            name.unwrap(),
            field(block, 100..108),
            field(block, 108..116),
            field(block, 116..124),
            archive[at + 512..at + 512 + size].to_vec(),
        ));
        at += 512 + size.div_ceil(512) * 512;
    }
    // Two empty blocks end the archive.
    assert_eq!(archive.len(), at + 1024);
    assert!(archive[at..].iter().all(|byte| *byte == 0));
    entries
}

#[test]
fn the_machine_id_goes_to_systemds_and_dbuss_files_as_root() {
    let id = "8aed3e38ba435c50deadb0896ace189b";
    let archive = machine_id_archive(id);
    assert_eq!(archive.len() % 512, 0);
    let content = format!("{id}\n").into_bytes();
    assert_eq!(
        entries(&archive),
        [
            ("etc/machine-id".into(), 0o644, 0, 0, content.clone()),
            ("var/lib/dbus/machine-id".into(), 0o644, 0, 0, content),
        ]
    );
}

/// Answer one request on the far end of an in-memory connection and return it, body included.
async fn serve(mut server: tokio::io::DuplexStream, response: &str) -> String {
    let mut request = Vec::new();
    let mut buffer = [0; 1024];
    loop {
        if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
            let length = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length: "))
                .map_or(0, |value| value.trim().parse::<usize>().unwrap());
            if request.len() >= end + 4 + length {
                break;
            }
        }
        let read = server.read(&mut buffer).await.unwrap();
        assert!(read > 0, "the client closed before sending its request");
        request.extend_from_slice(&buffer[..read]);
    }
    server.write_all(response.as_bytes()).await.unwrap();
    String::from_utf8(request).unwrap()
}

#[tokio::test]
async fn the_internet_network_is_connected_with_the_containers_mac_address() {
    let (client, server) = tokio::io::duplex(8192);
    let server = tokio::spawn(serve(
        server,
        "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n",
    ));
    let body = connect_request("bilskirnir-e", "02:00:00:00:00:01");
    engine_post(client, "/networks/bilskirnir-e-egress/connect", &body)
        .await
        .unwrap();
    let request = server.await.unwrap();
    // No version in the path: the daemon's own, which Docker 29 needs to use the address.
    assert!(
        request.starts_with("POST /networks/bilskirnir-e-egress/connect HTTP/1.1\r\n"),
        "{request}"
    );
    assert!(
        request
            .to_ascii_lowercase()
            .contains("content-type: application/json")
    );
    let (_, sent) = request.split_once("\r\n\r\n").unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(sent).unwrap(),
        json!({"Container": "bilskirnir-e", "EndpointConfig": {"MacAddress": "02:00:00:00:00:01"}})
    );
}

#[tokio::test]
async fn a_refused_connection_reports_dockers_message() {
    let (client, server) = tokio::io::duplex(8192);
    let body = r#"{"message":"network bilskirnir-e-egress not found"}"#;
    let response = format!(
        "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let server = tokio::spawn(async move { serve(server, &response).await });
    let error = engine_post(
        client,
        "/networks/bilskirnir-e-egress/connect",
        &connect_request("bilskirnir-e", "02:00:00:00:00:01"),
    )
    .await
    .unwrap_err();
    server.await.unwrap();
    assert_eq!(
        error.to_string(),
        "Docker: network bilskirnir-e-egress not found (404 Not Found)"
    );
}
