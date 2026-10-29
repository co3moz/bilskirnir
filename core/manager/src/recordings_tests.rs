//! Pins down what a recording reports about its video: the helper's status as it is, and a
//! dashboard link to the video file when there is one, an MP4 or the MJPEG AVI fallback alike. Also
//! which recording and export requests are refused.
use super::*;

fn recording() -> Recording {
    Recording {
        id: "abc".into(),
        container_id: "env".into(),
        name: "Level 1".into(),
        fps: 2,
        created_at: 1,
    }
}

#[test]
fn a_video_gets_a_download_link_for_its_own_container() {
    let path = "/tmp/thoric-recordings/abc/recording.mp4";
    let described = describe(
        &recording(),
        json!({"state": "finished", "frames": 4, "video": {"path": path, "codec": "H.264", "container": "MP4"}}),
    );
    assert_eq!(described["recording_id"], "abc");
    assert_eq!(described["state"], "finished");
    assert_eq!(described["directory"], "/tmp/thoric-recordings/abc");
    assert_eq!(described["video"]["codec"], "H.264");
    assert_eq!(
        described["video"]["download_url"],
        "/api/containers/env/files/download?path=/tmp/thoric-recordings/abc/recording.mp4"
    );
}

#[test]
fn the_avi_fallback_keeps_its_reason_beside_the_link() {
    let described = describe(
        &recording(),
        json!({"state": "finished", "video": {
            "path": "/tmp/thoric-recordings/abc/recording.avi",
            "codec": "MJPEG",
            "fallback": "GStreamer elements missing in this desktop image: x264enc",
        }}),
    );
    assert_eq!(
        described["video"]["fallback"],
        "GStreamer elements missing in this desktop image: x264enc"
    );
    assert!(
        described["video"]["download_url"]
            .as_str()
            .unwrap()
            .ends_with("/recording.avi")
    );
}

#[test]
fn no_link_without_a_video_file() {
    let failed = describe(
        &recording(),
        json!({"state": "finished", "video": {"error": "The recording has no frames"}}),
    );
    assert_eq!(
        failed["video"],
        json!({"error": "The recording has no frames"})
    );
    let running = describe(&recording(), json!({"state": "recording", "frames": 3}));
    assert!(running.get("video").is_none());
}

#[test]
fn recording_requests_are_validated() {
    let start = |value: serde_json::Value| {
        serde_json::from_value::<StartRecording>(value)
            .unwrap()
            .validate()
    };
    assert!(start(serde_json::json!({})).is_ok());
    assert!(
        start(
            serde_json::json!({"fps":5,"max_seconds":600,"max_width":320,"region":"0,0,100,100"})
        )
        .is_ok()
    );
    for bad in [
        serde_json::json!({"fps":0}),
        serde_json::json!({"fps":6}),
        serde_json::json!({"max_seconds":601}),
        serde_json::json!({"max_width":8}),
        serde_json::json!({"region":"1,2"}),
    ] {
        assert!(start(bad).is_err());
    }
    let export =
        |value: serde_json::Value| serde_json::from_value::<Export>(value).unwrap().validate();
    assert!(
        export(serde_json::json!({"destination":"/home/desktop/Downloads","name":"run.tar.gz","recording_ids":["a"]}))
            .is_ok()
    );
    assert!(
        export(serde_json::json!({"destination":"/home/desktop/Downloads","name":"run.tar.gz"}))
            .is_err()
    );
}
