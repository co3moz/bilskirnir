//! Pins down the screenshot options that are refused, the size read from a JPEG's header, and
//! where a screenshot may be saved with the download link it gets.
use super::*;
use crate::{files, uploads, windows};

#[test]
fn screenshot_options_and_jpeg_dimensions_are_validated() {
    let ok = Capture {
        max_width: Some(640),
        region: Some("10, 20,300,200".into()),
        window_id: Some("0x3a00007".into()),
    };
    assert_eq!(ok.region().unwrap(), Some([10, 20, 300, 200]));
    assert!(ok.validate().is_ok());
    for bad in [
        Capture {
            max_width: Some(8),
            ..Default::default()
        },
        Capture {
            region: Some("0,0,0,10".into()),
            ..Default::default()
        },
        Capture {
            region: Some("1,2,3".into()),
            ..Default::default()
        },
        Capture {
            window_id: Some("0xzz".into()),
            ..Default::default()
        },
        Capture {
            window_id: Some("0x123456789".into()),
            ..Default::default()
        },
    ] {
        assert!(bad.validate().is_err());
    }
    let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10];
    jpeg.extend([0u8; 14]);
    jpeg.extend([0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0xf0, 0x01, 0x40]);
    assert_eq!(jpeg_dimensions(&jpeg), Some((320, 240)));
    assert_eq!(jpeg_dimensions(&[0xff, 0xd8, 0xff, 0xd9]), None);
    assert!(windows::window_id("42").is_ok());
    assert!(windows::window_id("abc").is_err());
}

#[test]
fn screenshot_save_targets_and_download_links_are_shaped_for_users() {
    let save = |path: &str| SaveCapture {
        path: path.into(),
        replace: false,
        capture: Capture::default(),
    };
    assert_eq!(
        save("/home/desktop/Downloads/clock.jpg").target().unwrap(),
        ("/home/desktop/Downloads".into(), "clock.jpg".into())
    );
    for invalid in [
        "Downloads/clock.jpg",
        "/home/desktop/Downloads/",
        "/clock.jpg",
        "",
    ] {
        assert!(save(invalid).target().is_err(), "{invalid}");
    }
    // The directory part still has to pass the upload destination rules.
    let check = |target: &str, name: &str| uploads::check_target(&[], &[], target, name);
    assert!(check("/home/desktop/Downloads/../.ssh", "id_rsa").is_err());
    assert!(check("/etc", "clock.jpg").is_err());
    assert!(check("/home/desktop/Downloads", "clock.jpg").is_ok());
    assert_eq!(
        files::download_url("abc123", "/home/desktop/Downloads/saat 17:20.jpg"),
        "/api/containers/abc123/files/download?path=/home/desktop/Downloads/saat%2017%3A20.jpg"
    );
    assert_eq!(
        files::download_url("abc123", "/shared/ölçüm.tar.gz"),
        "/api/containers/abc123/files/download?path=/shared/%C3%B6l%C3%A7%C3%BCm.tar.gz"
    );
}
