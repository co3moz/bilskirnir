//! Pins down which audio capture and injection requests are refused, and with them those of
//! terminals and accessibility inspection, and the types downloaded files are sent as.
use super::*;
use crate::{files, terminal, windows};

#[test]
fn audio_terminal_download_and_inspection_requests_are_validated() {
    assert!(
        serde_json::from_value::<CaptureAudio>(serde_json::json!({}))
            .unwrap()
            .validate()
            .is_ok()
    );
    for bad in [
        serde_json::json!({"seconds":0}),
        serde_json::json!({"seconds":11}),
        serde_json::json!({"source":"line-in"}),
    ] {
        assert!(
            serde_json::from_value::<CaptureAudio>(bad)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    assert_eq!(
        serde_json::from_value::<InjectAudio>(serde_json::json!({"data_base64":"AAAA"}))
            .unwrap()
            .validate()
            .unwrap(),
        "browser-mic-sink"
    );
    assert!(
        serde_json::from_value::<InjectAudio>(
            serde_json::json!({"data_base64":"AAAA","target":"headphones"})
        )
        .unwrap()
        .validate()
        .is_err()
    );
    assert!(
        serde_json::from_value::<terminal::TerminalInput>(serde_json::json!({"text":"ls\n"}))
            .unwrap()
            .validate()
            .is_ok()
    );
    assert!(
        serde_json::from_value::<terminal::TerminalInput>(serde_json::json!({"text":""}))
            .unwrap()
            .validate()
            .is_err()
    );
    assert!(
        serde_json::from_value::<terminal::TerminalOutput>(serde_json::json!({"limit":0}))
            .unwrap()
            .validate()
            .is_err()
    );
    assert!(
        serde_json::from_value::<terminal::OpenTerminal>(serde_json::json!({"cwd":"relative"}))
            .unwrap()
            .validate()
            .is_err()
    );
    assert!(
        serde_json::from_value::<windows::Inspect>(serde_json::json!({"max_depth":21}))
            .unwrap()
            .validate()
            .is_err()
    );
    assert!(
        serde_json::from_value::<windows::Inspect>(serde_json::json!({"pid":12,"name":"Term"}))
            .unwrap()
            .validate()
            .is_ok()
    );
    assert_eq!(files::mime_type("shot.PNG"), "image/png");
    assert_eq!(files::mime_type("archive.tar.gz"), "application/gzip");
    assert_eq!(files::mime_type("binary"), "application/octet-stream");
}
