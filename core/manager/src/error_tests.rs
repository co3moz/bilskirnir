//! Pins down the status a request is answered with: a value refused by a check stays the
//! request's to fix (400), while Docker that cannot be reached, Docker's or a helper's own
//! failure, a command that ran out of time, the database and the disk get a 5xx, wherever in the
//! chain their source marked it, and every message stays the one the source gave.
use super::*;
use anyhow::Context;

fn answered(error: anyhow::Error) -> (StatusCode, String) {
    let Error(status, message) = error.into();
    (status, message)
}

#[test]
fn a_refused_value_stays_the_requests_to_fix() {
    let (status, message) = answered(
        anyhow::anyhow!("Script must contain 1 to 1000000 bytes").context("Invalid script"),
    );
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        message,
        "Invalid script: Script must contain 1 to 1000000 bytes"
    );
}

#[test]
fn a_fault_keeps_its_status_and_message_under_any_context() {
    let fault = Fault::error(
        StatusCode::SERVICE_UNAVAILABLE,
        "Docker: Cannot connect to the Docker daemon",
    );
    let (status, message) = answered(
        Err::<(), _>(fault)
            .context("Volume not found")
            .context("Creating the container")
            .unwrap_err(),
    );
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        message,
        "Creating the container: Volume not found: Docker: Cannot connect to the Docker daemon"
    );
}

#[test]
fn the_manager_failing_itself_is_an_internal_error() {
    let disk = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied");
    let (status, message) =
        answered(anyhow::Error::from(disk).context("Could not write the build context"));
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        message,
        "Could not write the build context: permission denied"
    );
    // The database keeps its own answers: a missing row is a missing record.
    let (status, _) = answered(rusqlite::Error::QueryReturnedNoRows.into());
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, message) = answered(rusqlite::Error::InvalidQuery.into());
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(message, rusqlite::Error::InvalidQuery.to_string());
}

#[tokio::test]
async fn a_task_that_did_not_finish_is_an_internal_error() {
    let task = tokio::spawn(std::future::pending::<()>());
    task.abort();
    let (status, _) = answered(anyhow::Error::from(task.await.unwrap_err()));
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn a_docker_answer_gets_the_status_of_what_it_means() {
    let status = |stderr: &str, code: Option<i32>| {
        let (status, message) = answered(docker_failure(stderr.as_bytes(), code));
        // The message is what it always was.
        assert_eq!(message, format!("Docker: {}", stderr.trim()));
        status
    };
    let cases: &[(&str, Option<i32>, StatusCode)] = &[
        (
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?\n",
            Some(1),
            StatusCode::SERVICE_UNAVAILABLE,
        ),
        (
            "permission denied while trying to connect to the Docker daemon socket at unix:///var/run/docker.sock",
            Some(1),
            StatusCode::SERVICE_UNAVAILABLE,
        ),
        // An object Docker does not have is the request's, as it always was.
        (
            "Error response from daemon: No such container: thoric-gone",
            Some(1),
            StatusCode::BAD_REQUEST,
        ),
        (
            "Error: No such object: thoric-gone",
            Some(1),
            StatusCode::BAD_REQUEST,
        ),
        (
            "Error response from daemon: get projects: no such volume",
            Some(1),
            StatusCode::BAD_REQUEST,
        ),
        (
            "Error response from daemon: network thoric-e-private not found",
            Some(1),
            StatusCode::BAD_REQUEST,
        ),
        // A name in use, or a container in the wrong state, conflicts.
        (
            "Error response from daemon: Conflict. The container name \"/thoric-e\" is already in use by container \"c0ffee\".",
            Some(1),
            StatusCode::CONFLICT,
        ),
        (
            "Error response from daemon: remove busy: volume is in use - [c0ffee]",
            Some(1),
            StatusCode::CONFLICT,
        ),
        (
            "Error response from daemon: container c0ffee is not running",
            Some(1),
            StatusCode::CONFLICT,
        ),
        // The daemon failing at what it was asked is its failure, also when a device is missing.
        (
            "Error response from daemon: failed to create task for container: OCI runtime create failed",
            Some(1),
            StatusCode::BAD_GATEWAY,
        ),
        (
            "docker: Error response from daemon: error gathering device information while adding custom device \"/dev/net/tun\": no such file or directory.",
            Some(125),
            StatusCode::BAD_GATEWAY,
        ),
        // A helper that crashed, could not run, or was killed failed the manager.
        (
            "Traceback (most recent call last):\n  File \"<string>\", line 1, in <module>\nKeyError: 'state'",
            Some(1),
            StatusCode::BAD_GATEWAY,
        ),
        (
            "OCI runtime exec failed: exec failed: unable to start container process: exec: \"python3\": executable file not found in $PATH: unknown",
            Some(127),
            StatusCode::BAD_GATEWAY,
        ),
        ("", Some(137), StatusCode::BAD_GATEWAY),
        ("", None, StatusCode::BAD_GATEWAY),
        // What a command in the container said about the request stays the request's.
        (
            "File operation failed: [Errno 2] No such file or directory: '/home/desktop/x'",
            Some(1),
            StatusCode::BAD_REQUEST,
        ),
        ("", Some(1), StatusCode::BAD_REQUEST),
    ];
    for (stderr, code, expected) in cases {
        assert_eq!(status(stderr, *code), *expected, "{stderr:?} {code:?}");
    }
}

#[test]
fn an_engine_answer_keeps_dockers_own_status_class() {
    let status = |code: StatusCode| answered(engine_failure(code, "Docker: x".into())).0;
    assert_eq!(status(StatusCode::CONFLICT), StatusCode::CONFLICT);
    assert_eq!(status(StatusCode::NOT_FOUND), StatusCode::BAD_REQUEST);
    assert_eq!(status(StatusCode::BAD_REQUEST), StatusCode::BAD_REQUEST);
    assert_eq!(
        status(StatusCode::INTERNAL_SERVER_ERROR),
        StatusCode::BAD_GATEWAY
    );
    assert_eq!(
        status(StatusCode::SERVICE_UNAVAILABLE),
        StatusCode::BAD_GATEWAY
    );
}

#[tokio::test]
async fn a_docker_command_that_runs_out_of_time_is_a_gateway_timeout() {
    #[cfg(windows)]
    let command = {
        let mut command = tokio::process::Command::new("powershell");
        command.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"]);
        command
    };
    #[cfg(not(windows))]
    let command = {
        let mut command = tokio::process::Command::new("sleep");
        command.arg("30");
        command
    };
    let error = crate::process::capture(
        command,
        std::time::Duration::from_millis(100),
        1024,
        crate::process::Overflow::Reject,
    )
    .await
    .unwrap_err();
    assert_eq!(
        answered(error),
        (
            StatusCode::GATEWAY_TIMEOUT,
            "Docker operation timed out".into()
        )
    );
}

#[cfg(unix)]
#[test]
fn a_helpers_failure_is_its_own_line_with_the_status_it_means() {
    use std::os::unix::process::ExitStatusExt;
    let helper = |stderr: &str, status: i32| {
        Error::helper(&std::process::Output {
            status: std::process::ExitStatus::from_raw(status),
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        })
    };
    // Exit codes are in the high byte of a wait status; a signal is in the low one.
    let exited = |code: i32| code << 8;
    let refused = helper("Not a directory: /home/desktop/notes\n", exited(1));
    assert_eq!(refused.0, StatusCode::BAD_REQUEST);
    assert_eq!(refused.1, "Not a directory: /home/desktop/notes");
    let conflict = helper("CONFLICT: Destination already exists.", exited(17));
    assert_eq!(conflict.0, StatusCode::CONFLICT);
    assert_eq!(conflict.1, "CONFLICT: Destination already exists.");
    for (stderr, status) in [
        (
            "Traceback (most recent call last):\nKeyError: 'x'",
            exited(1),
        ),
        ("exec: \"python3\": executable file not found", exited(127)),
        ("", 9),
    ] {
        assert_eq!(
            helper(stderr, status).0,
            StatusCode::BAD_GATEWAY,
            "{stderr}"
        );
    }
    let stopped = helper(
        "Error response from daemon: container c0ffee is not running",
        exited(1),
    );
    assert_eq!(stopped.0, StatusCode::CONFLICT);
}
