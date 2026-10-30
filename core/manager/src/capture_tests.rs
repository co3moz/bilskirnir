//! Pins down the screenshot options that are refused, the size read from a JPEG's header, where
//! a screenshot's pixels are on the desktop, and where a screenshot may be saved with the
//! download link it gets.
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

/// What xwininfo prints of the root window and of a window, as the screenshot wrapper passes it on.
const XWININFO: &str = "
xwininfo: Window id: 0x1e7 (the root window) (has no name)

  Absolute upper-left X:  0
  Absolute upper-left Y:  0
  Relative upper-left X:  0
  Relative upper-left Y:  0
  Width: 1920
  Height: 1080
  Depth: 24
  -geometry 1920x1080+0+0

xwininfo: Window id: 0x3a00007 \"Editor: notes.txt\"

  Absolute upper-left X:  400
  Absolute upper-left Y:  130
  Relative upper-left X:  0
  Relative upper-left Y:  0
  Width: 800
  Height: 600
  -geometry 800x600+400+130
";

/// A scaled, cropped or window screenshot told the agent nothing of where its pixels were, so a
/// point read off it missed in desktop_input. Each result says how its pixels map back.
#[test]
fn a_screenshot_says_where_its_pixels_are_on_the_desktop() {
    let rects = rectangles(XWININFO);
    assert_eq!(rects, vec![[0, 0, 1920, 1080], [400, 130, 800, 600]]);
    let desktop = Some((1920, 1080));
    let capture = |max_width: Option<u32>, region: Option<&str>, window: bool| Capture {
        max_width,
        region: region.map(str::to_owned),
        window_id: window.then(|| "0x3a00007".to_owned()),
    };
    for (options, rects, width, expected) in [
        // Full resolution, and a max_width wider than the desktop, which scales nothing.
        (
            capture(None, None, false),
            &rects[..1],
            1920,
            (Some(1.0), Some((0, 0))),
        ),
        (
            capture(Some(2560), None, false),
            &rects[..1],
            1920,
            (Some(1.0), Some((0, 0))),
        ),
        (
            capture(Some(960), None, false),
            &rects[..1],
            960,
            (Some(0.5), Some((0, 0))),
        ),
        // A region is cropped first, then scaled; one past the desktop's edge is clipped to it.
        (
            capture(Some(300), Some("100,50,600,400"), false),
            &rects[..1],
            300,
            (Some(0.5), Some((100, 50))),
        ),
        (
            capture(None, Some("1800,0,400,100"), false),
            &rects[..1],
            120,
            (Some(1.0), Some((1800, 0))),
        ),
        // A window starts where it is, and a region within it from there.
        (
            capture(Some(400), None, true),
            &rects[..],
            400,
            (Some(0.5), Some((400, 130))),
        ),
        (
            capture(None, Some("10,20,100,50"), true),
            &rects[..],
            100,
            (Some(1.0), Some((410, 150))),
        ),
        // Without xwininfo a window's place is unknown, and so is a scale that max_width chose.
        (capture(Some(400), None, true), &[][..], 400, (None, None)),
        (capture(None, None, true), &[][..], 800, (Some(1.0), None)),
        (
            capture(Some(960), None, false),
            &[][..],
            960,
            (None, Some((0, 0))),
        ),
    ] {
        let placed = place(&options, rects, width);
        assert_eq!(
            (placed.scale, placed.origin),
            expected,
            "{:?} {:?} {}",
            options.max_width,
            options.region,
            options.window_id.is_some()
        );
        assert_eq!(placed.desktop, desktop.filter(|_| !rects.is_empty()));
    }
    let shot = Screenshot {
        jpeg: vec![0xff, 0xd8],
        width: 960,
        height: 540,
        captured_at: 7,
        placement: place(&capture(Some(960), None, false), &rects, 960),
    };
    assert_eq!(
        shot.summary(),
        json!({"width": 960, "height": 540, "captured_at": 7, "bytes": 2, "scale": 0.5,
            "origin_x": 0, "origin_y": 0, "desktop_width": 1920, "desktop_height": 1080})
    );
}
