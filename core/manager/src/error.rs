use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
#[derive(Debug)]
pub struct Error(pub StatusCode, pub String);
pub type Result<T> = std::result::Result<T, Error>;
/// What a request is answered with. A failure its source marked as a [`Fault`], anywhere in the
/// chain under the context added on the way up, keeps the fault's status; the database, the disk
/// and a task that panicked fail the manager itself (500). Anything else, which is mostly a
/// request's value refused by a check, is the request's to fix (400). The message is the whole
/// chain either way, except for a database failure.
impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        for cause in e.chain() {
            if let Some(fault) = cause.downcast_ref::<Fault>() {
                return Self(fault.status, format!("{e:#}"));
            }
            if let Some(sql) = cause.downcast_ref::<rusqlite::Error>() {
                return if matches!(sql, rusqlite::Error::QueryReturnedNoRows) {
                    missing()
                } else {
                    Self(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
                };
            }
            if cause.is::<std::io::Error>() || cause.is::<tokio::task::JoinError>() {
                return Self(StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}"));
            }
        }
        Self(StatusCode::BAD_REQUEST, format!("{e:#}"))
    }
}
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({"error":self.1}))).into_response()
    }
}
pub fn missing() -> Error {
    Error(StatusCode::NOT_FOUND, "Record not found".into())
}

impl Error {
    /// A helper's failure, run through `docker exec`, with the one line it printed as the message:
    /// a `CONFLICT:` line is a conflict and anything else the helper said is the request's to
    /// fix, unless the answer shows that Docker or the helper itself failed ([`failure_status`]).
    pub fn helper(output: &std::process::Output) -> Self {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let status = failure_status(&message, output.status.code()).unwrap_or(
            if message.starts_with("CONFLICT:") {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_REQUEST
            },
        );
        Self(status, message)
    }
}

/// An error whose source knows a better status than the 400 of a refused request: Docker that
/// cannot be reached (503), a Docker command or a helper that failed (502) or ran out of time
/// (504), or Docker's refusal of a request that conflicts with an object's state (409). Its
/// source returns it as the error itself, with the message that source always gave, and [`Error`]
/// finds it under any context added on the way.
#[derive(Debug)]
pub struct Fault {
    status: StatusCode,
    message: String,
}
impl Fault {
    pub fn error(status: StatusCode, message: impl Into<String>) -> anyhow::Error {
        Self {
            status,
            message: message.into(),
        }
        .into()
    }
}
impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Fault {}

/// The error of a Docker CLI command that exited unsuccessfully: `Docker: ` and what it printed
/// on stderr, a [`Fault`] when [`failure_status`] finds one.
pub fn docker_failure(stderr: &[u8], code: Option<i32>) -> anyhow::Error {
    let stderr = String::from_utf8_lossy(stderr);
    let message = format!("Docker: {}", stderr.trim());
    match failure_status(&stderr, code) {
        Some(status) => Fault::error(status, message),
        None => anyhow::anyhow!(message),
    }
}

/// The error of an Engine API answer that is not a success, by Docker's own status: a conflict
/// stays one and Docker's own failure is a bad gateway, while any other refusal is the request's,
/// as the CLI's are.
pub fn engine_failure(status: StatusCode, message: String) -> anyhow::Error {
    if status == StatusCode::CONFLICT {
        Fault::error(StatusCode::CONFLICT, message)
    } else if status.is_server_error() {
        Fault::error(StatusCode::BAD_GATEWAY, message)
    } else {
        anyhow::anyhow!(message)
    }
}

/// The status of what a failed Docker CLI command, or the command it ran in a container, printed
/// on stderr and the code it exited with, or `None` when the answer is about the request: what
/// [`answer_status`] reads in the answer, or a failure of the manager when the command could not
/// run in the container (126, 127) or was killed by a signal.
pub fn failure_status(stderr: &str, code: Option<i32>) -> Option<StatusCode> {
    answer_status(stderr)
        .or_else(|| (!matches!(code, Some(0..=125))).then_some(StatusCode::BAD_GATEWAY))
}

/// The status of a failed command's answer on stderr, or `None` when it is about the request.
/// Docker that cannot be reached is unavailable, and a helper that crashed with Python's
/// traceback failed the manager. The daemon's own answers keep their meaning where it has one
/// for the request: an object in use, or one in the wrong state, is a conflict, and one it does
/// not have is the request's, as before; any other is the daemon's failure.
pub fn answer_status(stderr: &str) -> Option<StatusCode> {
    let text = stderr.trim().to_lowercase();
    if UNREACHABLE.iter().any(|sign| text.contains(sign)) {
        return Some(StatusCode::SERVICE_UNAVAILABLE);
    }
    if text.contains("traceback (most recent call last)") {
        return Some(StatusCode::BAD_GATEWAY);
    }
    let answer = text.contains("error response from daemon") || text.starts_with("error: no such");
    if !answer {
        return None;
    }
    if CONFLICT.iter().any(|sign| text.contains(sign)) {
        Some(StatusCode::CONFLICT)
    } else if MISSING.iter().any(|sign| text.contains(sign)) || text.ends_with(" not found") {
        None
    } else {
        Some(StatusCode::BAD_GATEWAY)
    }
}

/// How the Docker CLI says it cannot reach the daemon, lowercase.
const UNREACHABLE: &[&str] = &[
    "cannot connect to the docker daemon",
    "error during connect",
    "while trying to connect to the docker daemon",
];
/// How the daemon refuses a request that conflicts with an object's state, lowercase: a name or
/// an object in use, or a container that is not running.
const CONFLICT: &[&str] = &[
    "conflict",
    "already in use",
    "is in use",
    "already exists",
    "is not running",
    "is restarting",
    "is paused",
];
/// How the daemon says it does not have an object, lowercase.
const MISSING: &[&str] = &[
    "no such container",
    "no such image",
    "no such volume",
    "no such network",
    "no such object",
    "no such exec",
];

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
