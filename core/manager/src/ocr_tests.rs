//! Pins down what `find_text` takes, how much a capture is enlarged before Tesseract reads it,
//! and how the manager runs Tesseract: a PGM on its standard input, sparse text in English, an
//! answer that names the packages when it is missing, its own message when it fails, and a bound
//! on how long it may take.
use super::*;

fn request(value: Value) -> FindText {
    serde_json::from_value(value).unwrap()
}

#[test]
fn a_request_names_a_text_a_region_and_a_confidence_within_their_limits() {
    let all = request(json!({}));
    assert_eq!(
        (all.level, all.min_confidence, all.case_sensitive),
        (Level::Line, 50, false)
    );
    assert!(all.validate().is_ok());
    assert!(
        request(json!({"text":"Save project","region":"0,0,640,480","level":"word","min_confidence":0,"case_sensitive":true}))
            .validate()
            .is_ok()
    );
    for bad in [
        json!({"text":""}),
        json!({"text":" \n "}),
        json!({"text":"x".repeat(TEXT_LIMIT + 1)}),
        json!({"min_confidence":101}),
        json!({"region":"0,0,0,10"}),
        json!({"region":"1,2,3"}),
    ] {
        assert!(request(bad.clone()).validate().is_err(), "{bad}");
    }
    for unknown in [json!({"level":"page"}), json!({"window_id":"0x1"})] {
        assert!(serde_json::from_value::<FindText>(unknown).is_err());
    }
}

#[test]
fn small_captures_are_enlarged_twice_and_large_ones_less_or_not_at_all() {
    assert_eq!(enlargement(1920, 1080), 2.0);
    assert_eq!(enlargement(320, 200), 2.0);
    assert_eq!(enlargement(2560, 1440), 1.5);
    assert_eq!(enlargement(3840, 2160), 1.0);
    assert_eq!(enlargement(7680, 4320), 1.0);
}

#[tokio::test]
async fn a_missing_container_is_refused_before_anything_runs() {
    let app = crate::tests::app(false);
    let error = find_text(&app, "missing", &request(json!({"text":"Play"})))
        .await
        .unwrap_err();
    assert_eq!(error.0, StatusCode::NOT_FOUND);
    let error = find_text(&app, "missing", &request(json!({"text":""})))
        .await
        .unwrap_err();
    assert_eq!(error.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_manager_without_tesseract_says_which_packages_it_needs() {
    let error = tesseract(
        "/nonexistent/thoric-tesseract",
        b"P5\n1 1\n255\n\0".to_vec(),
        "11",
        TIMEOUT,
    )
    .await
    .unwrap_err();
    assert_eq!(error.0, StatusCode::NOT_IMPLEMENTED);
    assert!(
        error.1.contains("tesseract-ocr") && error.1.contains("tesseract-ocr-eng"),
        "{}",
        error.1
    );
}

/// A Tesseract that is a shell script, in a new directory of its own.
#[cfg(unix)]
fn fake_tesseract(script: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("thoric-ocr-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    let program = dir.join("tesseract");
    std::fs::write(&program, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    program
}

#[cfg(unix)]
#[tokio::test]
async fn tesseract_reads_the_image_on_stdin_and_answers_on_stdout() {
    // It prints its arguments, its thread limit and what it was given.
    let program = fake_tesseract("echo \"$*\"; echo \"$OMP_THREAD_LIMIT\"; head -c 2");
    let image = Gray {
        width: 2,
        height: 1,
        pixels: vec![0, 255],
    }
    .pgm();
    let output = tesseract(program.to_str().unwrap(), image, "3", TIMEOUT)
        .await
        .unwrap();
    assert_eq!(output, "stdin stdout --psm 3 -l eng tsv\n1\nP5");
    std::fs::remove_dir_all(program.parent().unwrap()).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_failing_or_slow_tesseract_is_an_error_with_its_own_words() {
    let failing = fake_tesseract(
        "echo 'Error opening data file eng.traineddata' >&2; echo \"Failed loading language 'eng'\" >&2; exit 1",
    );
    let error = tesseract(failing.to_str().unwrap(), vec![0; 1 << 20], "11", TIMEOUT)
        .await
        .unwrap_err();
    assert_eq!(error.0, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        error.1.contains("Failed loading language 'eng'"),
        "{}",
        error.1
    );
    let slow = fake_tesseract("sleep 5");
    let started = std::time::Instant::now();
    let error = tesseract(
        slow.to_str().unwrap(),
        vec![0; 16],
        "11",
        Duration::from_millis(300),
    )
    .await
    .unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(4));
    assert!(error.1.contains("longer than 0.3 seconds"), "{}", error.1);
    for program in [failing, slow] {
        std::fs::remove_dir_all(program.parent().unwrap()).unwrap();
    }
}

#[test]
fn the_manager_image_installs_the_packages_the_missing_answer_names() {
    // The published manager is where Tesseract is expected; the answer of one without it names
    // these packages, so the image must keep installing them.
    let dockerfile = include_str!("../Dockerfile");
    for package in ["tesseract-ocr", "tesseract-ocr-eng"] {
        assert!(
            dockerfile.split_whitespace().any(|word| word == package),
            "core/manager/Dockerfile must install {package}"
        );
        assert!(missing().1.contains(package));
    }
}
