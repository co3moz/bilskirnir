//! Pins down which folders are handed to the desktop user before a container starts: only those
//! root owns on the way from the home to a mount point, never past a link, a file or another
//! volume, each read from the start of Docker's archive alone and written back with its own mode
//! and time.
use super::*;

fn mount(target: &str) -> Mount {
    Mount {
        volume: "data".into(),
        target: target.into(),
        readonly: false,
    }
}

#[test]
fn a_way_holds_the_homes_folders_above_a_mount_point() {
    let mounts = [
        mount("/home/desktop/.config/chrome-agent"),
        mount("/home/desktop/Unity/Cache/Deep"),
        mount("/home/desktop/Projects"),
        mount("/unity-install-location"),
        mount("/workspace/a/b"),
        mount("/home/desktopish/a/b"),
    ];
    assert_eq!(
        ways(&mounts),
        [vec![".config"], vec!["Unity", "Unity/Cache"]]
    );
}

#[test]
fn another_volumes_folder_ends_a_way() {
    let mounts = [
        mount("/home/desktop/data"),
        mount("/home/desktop/data/sub/x"),
        mount("/home/desktop/.cache/a/b"),
        mount("/home/desktop/.cache/a"),
    ];
    assert_eq!(ways(&mounts), [vec![".cache"], vec![".cache"]]);
    // A volume at the home holds every folder in it.
    let mounts = [mount("/home/desktop"), mount("/home/desktop/.config/x")];
    assert!(ways(&mounts).is_empty());
}

/// A folder's header as Docker 29 writes it for `docker cp CONTAINER:/home/desktop/.config -`.
fn docker_header(name: &str, uid: &[u8; 7], typeflag: u8) -> Vec<u8> {
    let mut header = vec![0u8; 512];
    header[..name.len()].copy_from_slice(name.as_bytes());
    header[100..107].copy_from_slice(b"0000755");
    header[108..115].copy_from_slice(uid);
    header[116..123].copy_from_slice(uid);
    header[124..135].copy_from_slice(b"00000000000");
    header[136..147].copy_from_slice(b"15265505365");
    header[148..155].copy_from_slice(b"010603\0");
    header[155] = b' ';
    header[156] = typeflag;
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    header
}

#[test]
fn a_folders_owner_mode_and_time_come_from_the_start_of_dockers_archive() {
    let root = Entry {
        directory: true,
        uid: 0,
        mode: 0o755,
        mtime: 0o15265505365,
    };
    let mut archive = docker_header(".config/", b"0000000", b'5');
    // What follows the folder's own entry is never needed.
    archive.extend(docker_header(".config/chrome-agent/", b"0000000", b'5'));
    assert_eq!(first_entry(&archive).unwrap(), Some(root.clone()));
    let desktop = docker_header("desktop/", b"0001751", b'5');
    assert_eq!(first_entry(&desktop).unwrap().unwrap().uid, 1001);
    let link = docker_header("link", b"0001751", b'2');
    assert!(!first_entry(&link).unwrap().unwrap().directory);
    // An extension header describes the entry after it.
    let records = b"30 mtime=1784000000.123456789\n";
    let mut extended = docker_header("PaxHeaders.0/.config", b"0000000", b'x');
    extended[124..135].copy_from_slice(format!("{:011o}", records.len()).as_bytes());
    extended.extend_from_slice(records);
    extended.resize(1024, 0);
    extended.extend(docker_header(".config/", b"0000000", b'5'));
    assert_eq!(first_entry(&extended).unwrap(), Some(root));
    // An archive that ends before an entry has none, and anything else is not read as one.
    for ended in [&[][..], &[0; 1024], &archive[..511], &extended[..1024]] {
        assert_eq!(first_entry(ended).unwrap(), None);
    }
    assert!(first_entry(&[b'x'; 512]).is_err());
}

/// The entries of an archive of folders: name, type, mode, owner, group and time.
fn folders(archive: &[u8]) -> Vec<(String, u8, u32, u32, u32, u64)> {
    let field = |block: &[u8], range: std::ops::Range<usize>| {
        let text = std::str::from_utf8(&block[range]).unwrap();
        u64::from_str_radix(text.trim_end_matches(['\0', ' ']), 8).unwrap()
    };
    let mut entries = Vec::new();
    let mut at = 0;
    while archive[at..at + 512].iter().any(|byte| *byte != 0) {
        let block = &archive[at..at + 512];
        let sum: u64 = block
            .iter()
            .enumerate()
            .map(|(i, byte)| u64::from(if (148..156).contains(&i) { b' ' } else { *byte }))
            .sum();
        assert_eq!(field(block, 148..156), sum, "checksum");
        assert_eq!(&block[257..265], b"ustar\x0000");
        assert_eq!(field(block, 124..136), 0, "a folder holds no data");
        let name = block[..100].split(|byte| *byte == 0).next().unwrap();
        entries.push((
            String::from_utf8(name.to_vec()).unwrap(),
            block[156],
            field(block, 100..108) as u32,
            field(block, 108..116) as u32,
            field(block, 116..124) as u32,
            field(block, 136..148),
        ));
        at += 512;
    }
    // Two empty blocks end the archive.
    assert_eq!(archive.len(), at + 1024);
    assert!(archive[at..].iter().all(|byte| *byte == 0));
    entries
}

#[test]
fn a_handed_over_folder_keeps_its_mode_and_time() {
    let entry = |mode, mtime| Entry {
        directory: true,
        uid: 0,
        mode,
        mtime,
    };
    let archive = hand_over_archive(&[
        (".config".into(), entry(0o755, 1_784_000_000)),
        ("Unity/Cache".into(), entry(0o2770, 1)),
        // A name a header cannot hold stays as it is, and a time it cannot hold is cut.
        ("x".repeat(101), entry(0o755, 1)),
        ("late".into(), entry(0o700, u64::MAX)),
    ]);
    assert_eq!(
        folders(&archive),
        [
            (".config".into(), b'5', 0o755, 1001, 1001, 1_784_000_000),
            ("Unity/Cache".into(), b'5', 0o2770, 1001, 1001, 1),
            ("late".into(), b'5', 0o700, 1001, 1001, 0o77777777777),
        ]
    );
}

/// A stand-in Docker CLI that archives the folders `tars` holds, the first one followed by
/// endless zeros, records each call and keeps what is copied into the container.
#[cfg(unix)]
fn docker(app: &mut std::sync::Arc<App>, tars: &[(&str, Vec<u8>)]) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("thoric-parents-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut cases = String::new();
    for (index, (folder, tar)) in tars.iter().enumerate() {
        std::fs::write(dir.join(format!("{index}.tar")), tar).unwrap();
        // The rest of a folder's archive can be as large as everything in it.
        let rest = if index == 0 {
            "; exec cat /dev/zero"
        } else {
            ""
        };
        cases += &format!(
            "  'cp thoric-e:/home/desktop/{folder} -') cat '{}/{index}.tar'{rest} ;;\n",
            dir.display()
        );
    }
    let docker = dir.join("docker");
    std::fs::write(
        &docker,
        format!(
            "#!/bin/sh\necho \"$*\" >> '{dir}/calls'\ncase \"$*\" in\n{cases}  'cp - thoric-e:/home/desktop') cat > '{dir}/written.tar' ;;\n  *) echo \"Error response from daemon: unexpected $*\" >&2; exit 1 ;;\nesac\n",
            dir = dir.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::sync::Arc::get_mut(app).unwrap().config.docker_bin = docker.display().to_string();
    dir
}

#[cfg(unix)]
fn container(targets: &[&str]) -> Container {
    let mut spec: ContainerSpec =
        serde_json::from_value(serde_json::json!({"name": "Agent", "image": "sha256:a"})).unwrap();
    spec.mounts = targets.iter().map(|target| mount(target)).collect();
    Container {
        id: "e".into(),
        container: "thoric-e".into(),
        private_network: "thoric-e-private".into(),
        egress_network: "thoric-e-egress".into(),
        source_image_id: "sha256:a".into(),
        created_at: 0,
        spec,
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    }
}

#[cfg(unix)]
#[tokio::test]
async fn roots_folders_above_mount_points_go_to_the_desktop_user_before_the_start() {
    let mut app = crate::test_support::app(false);
    let dir = docker(
        &mut app,
        &[
            (".config", docker_header(".config/", b"0000000", b'5')),
            ("Unity", docker_header("Unity/", b"0001751", b'5')),
            ("Unity/Cache", docker_header("Cache/", b"0000000", b'5')),
            ("link", docker_header("link", b"0000000", b'2')),
        ],
    );
    let env = container(&[
        "/home/desktop/.config/chrome-agent",
        "/home/desktop/.config/other",
        "/home/desktop/Unity/Cache/Deep",
        // Nothing past a link is read: the stand-in refuses `link/a`.
        "/home/desktop/link/a/b",
        "/home/desktop/Projects",
    ]);
    tokio::time::timeout(Duration::from_secs(30), app.hand_over_mount_parents(&env))
        .await
        .expect("only the start of an archive is read")
        .unwrap();
    let calls = std::fs::read_to_string(dir.join("calls")).unwrap();
    assert_eq!(
        calls.lines().collect::<Vec<_>>(),
        [
            "cp thoric-e:/home/desktop/.config -",
            "cp thoric-e:/home/desktop/Unity -",
            "cp thoric-e:/home/desktop/Unity/Cache -",
            "cp thoric-e:/home/desktop/link -",
            // Never `--archive`, which gives the folders the image's user: root in the images
            // Thoric builds.
            "cp - thoric-e:/home/desktop",
        ]
    );
    let written = std::fs::read(dir.join("written.tar")).unwrap();
    assert_eq!(
        folders(&written),
        [
            (".config".into(), b'5', 0o755, 1001, 1001, 0o15265505365),
            ("Unity/Cache".into(), b'5', 0o755, 1001, 1001, 0o15265505365),
        ]
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn nothing_is_written_when_the_desktop_user_owns_the_way() {
    let mut app = crate::test_support::app(false);
    let dir = docker(
        &mut app,
        &[(".config", docker_header(".config/", b"0001751", b'5'))],
    );
    app.hand_over_mount_parents(&container(&["/home/desktop/.config/chrome-agent"]))
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("calls")).unwrap(),
        "cp thoric-e:/home/desktop/.config -\n"
    );
    // Without a way there is nothing to read either.
    let mut app = crate::test_support::app(false);
    let none = docker(&mut app, &[]);
    app.hand_over_mount_parents(&container(&["/home/desktop/Projects", "/srv/a/b"]))
        .await
        .unwrap();
    assert!(!none.join("calls").exists());
    // A folder Docker cannot read stops the container's creation with Docker's message.
    let error = app
        .hand_over_mount_parents(&container(&["/home/desktop/.config/chrome-agent"]))
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unexpected cp thoric-e:/home/desktop/.config -"),
        "{error}"
    );
    std::fs::remove_dir_all(dir).unwrap();
    std::fs::remove_dir_all(none).unwrap();
}
