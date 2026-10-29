//! Pins down how a screenshot becomes pixels: color JPEGs turn gray by luma, a header claiming
//! more pixels than any desktop is refused before decoding, enlarging keeps edges where they were,
//! and a change is counted only where pixels differ by more than the threshold.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};

/// 16x8, red on the left half and blue on the right, as desktop-screenshot's ImageMagick writes it.
const RGB: &str = "/9j/4AAQSkZJRgABAQAAAAAAAAD/2wBDAAEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQH/2wBDAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQH/wAARCAAIABADAREAAhEBAxEB/8QAFAABAAAAAAAAAAAAAAAAAAAACf/EABQQAQAAAAAAAAAAAAAAAAAAAAD/xAAWAQEBAQAAAAAAAAAAAAAAAAALCQr/xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oADAMBAAIRAxEAPwAX0p2/gQ5VAa+//9k=";
/// 16x8, white on the left half and black on the right, with one gray component.
const GRAY: &str = "/9j/4AAQSkZJRgABAQAAAAAAAAD/2wBDAAIBAQEBAQIBAQECAgICAgQDAgICAgUEBAMEBgUGBgYFBgYGBwkIBgcJBwYGCAsICQoKCgoKBggLDAsKDAkKCgr/wAALCAAIABABAREA/8QAFQABAQAAAAAAAAAAAAAAAAAACQr/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/9oACAEBAAA/AH8QBv/Z";

fn image(width: usize, height: usize, pixels: &[u8]) -> Gray {
    Gray {
        width,
        height,
        pixels: pixels.to_vec(),
    }
}

#[test]
fn a_color_screenshot_turns_gray_by_luma_and_a_gray_one_stays_as_it_is() {
    let color = Gray::from_jpeg(&STANDARD.decode(RGB).unwrap()).unwrap();
    assert_eq!(
        (color.width, color.height, color.pixels.len()),
        (16, 8, 128)
    );
    // Red is 77 of 255 by luma and blue 29, give or take what JPEG keeps of them.
    assert!(color.pixels[0].abs_diff(77) <= 4, "{}", color.pixels[0]);
    assert!(color.pixels[15].abs_diff(29) <= 4, "{}", color.pixels[15]);
    let gray = Gray::from_jpeg(&STANDARD.decode(GRAY).unwrap()).unwrap();
    assert_eq!((gray.width, gray.height), (16, 8));
    assert!(
        gray.pixels[0] > 240 && gray.pixels[15] < 15,
        "{:?}",
        gray.pixels
    );
}

#[test]
fn a_header_larger_than_any_desktop_or_bytes_that_are_no_jpeg_are_refused() {
    let mut huge = STANDARD.decode(GRAY).unwrap();
    // The start of frame holds the height, then the width: claim 65535 by 65535.
    let frame = huge.windows(2).position(|w| w == [0xff, 0xc0]).unwrap();
    huge[frame + 5..frame + 9].copy_from_slice(&[0xff, 0xff, 0xff, 0xff]);
    let error = Gray::from_jpeg(&huge).unwrap_err().to_string();
    assert!(error.contains("65535x65535"), "{error}");
    let error = crate::error::Error::from(Gray::from_jpeg(b"not a jpeg").unwrap_err());
    // What the desktop's screenshot helper answered is its failure, not the request's.
    assert_eq!(error.0, StatusCode::BAD_GATEWAY, "{}", error.1);
    assert!(Gray::from_jpeg(&[]).is_err());
}

#[test]
fn enlarging_keeps_edges_in_place_and_a_factor_of_one_changes_nothing() {
    let edge = image(4, 1, &[0, 0, 255, 255]);
    assert_eq!(edge.enlarged(1.0), edge);
    let double = edge.enlarged(2.0);
    assert_eq!((double.width, double.height), (8, 2));
    assert_eq!(&double.pixels[..8], &[0, 0, 0, 64, 191, 255, 255, 255]);
    assert_eq!(double.pixels[..8], double.pixels[8..]);
    let wider = image(3, 2, &[10, 20, 30, 40, 50, 60]).enlarged(1.5);
    assert_eq!((wider.width, wider.height), (5, 3));
    assert_eq!(wider.pixels[0], 10);
    assert_eq!(*wider.pixels.last().unwrap(), 60);
}

#[test]
fn a_pgm_carries_the_size_before_the_pixels() {
    assert_eq!(image(2, 1, &[7, 200]).pgm(), b"P5\n2 1\n255\n\x07\xc8");
}

#[test]
fn a_change_counts_pixels_beyond_the_threshold_and_every_pixel_when_the_size_differs() {
    let before = image(4, 1, &[0, 100, 200, 255]);
    let after = image(4, 1, &[10, 100, 150, 0]);
    assert_eq!(before.changed_percent(&before, 0), 0.0);
    assert_eq!(before.changed_percent(&after, 24), 50.0);
    assert_eq!(before.changed_percent(&after, 5), 75.0);
    assert_eq!(before.changed_percent(&image(2, 2, &[0; 4]), 24), 100.0);
}
