//! A folder downloaded as one `.tar.gz`, from a running desktop or from a volume: the file
//! download routes answer a folder's path with it. tar reads the files while the archive streams,
//! so its size is not known up front; as with a volume export, a job records the transfer, so its
//! progress and any error stay visible after the browser's download has begun.
use crate::{
    App,
    archives::{send_archive, stall, transfer_permit},
    auth,
    error::{Error, Result},
    files::percent_encode,
    jobs,
};
use anyhow::Context;
use axum::{
    body::Bytes,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use std::{process::Stdio, sync::Arc};
use tokio::sync::mpsc;
use tracing::Instrument;

/// Opens the folder without following symlinks on its path and runs tar there.
const SCRIPT: &str = include_str!("../helpers/archive_folder.py");
/// The job a folder download is recorded as.
pub(crate) const KIND: &str = "folder-download";

/// What a download's path names, as `file_ops.py stat` describes it.
#[derive(Debug, PartialEq)]
pub(crate) enum Target {
    /// A regular file of this many bytes, which downloads as it is.
    File(u64),
    /// A folder, which downloads as a `.tar.gz`.
    Folder,
}

pub(crate) fn target(entry: &Value) -> Result<Target> {
    match entry["type"].as_str() {
        // The stat tried to open the file as the desktop user; a file it cannot read would
        // otherwise be answered with headers and then a body cut short.
        Some("file") if entry["readable"] == false => Err(Error(
            StatusCode::FORBIDDEN,
            "The desktop user cannot read the file".into(),
        )),
        Some("file") => Ok(Target::File(
            entry["size"].as_u64().context("Missing file size")?,
        )),
        Some("directory") => Ok(Target::Folder),
        _ => Err(Error(
            StatusCode::BAD_REQUEST,
            "Only regular files and folders can be downloaded".into(),
        )),
    }
}

/// The command that archives the folder at `path`, run where the folder is.
pub(crate) fn command(path: &str) -> Vec<String> {
    ["python3", "-c", SCRIPT, path].map(str::to_owned).to_vec()
}

/// Where an archive is made.
pub(crate) struct Source {
    /// The `docker` arguments that run [`command`] with stdin attached. Closing stdin stops tar.
    pub args: Vec<String>,
    /// The helper container the arguments start, removed when the download stops early.
    pub helper: Option<String>,
    /// The resource lock held from before the helper starts until Docker has it: a volume,
    /// which `docker run` would create anew after a delete slipped in between the check and
    /// the run. A folder in a desktop needs none.
    pub lock: Option<String>,
    /// The folder as the job log names it.
    pub label: String,
    /// The folder's path, for the job's result.
    pub path: String,
}

/// `Content-Disposition` for the archive of the folder `name`: an ASCII name for clients that
/// read only `filename`, the folder's own for those that read `filename*`.
pub(crate) fn disposition(name: &str) -> String {
    let plain: String = name
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | ' ' => c,
            _ => '_',
        })
        .collect();
    format!(
        "attachment; filename=\"{plain}.tar.gz\"; filename*=UTF-8''{}.tar.gz",
        percent_encode(name)
    )
}

/// Answer with the archive of the folder `name` that `source` makes, recorded as a job on
/// `resource`. The job does not hold the resource's lock, as a file download does not past its
/// start.
pub(crate) fn respond(
    app: Arc<App>,
    resource: String,
    name: &str,
    source: Source,
    token: auth::Session,
) -> Result<Response> {
    let permit = transfer_permit(&app)?;
    let (body, chunks) = mpsc::channel::<std::io::Result<Bytes>>(4);
    let span = tracing::Span::current();
    let (job_id, outcome) =
        jobs::start(app, KIND, resource, false, move |app, job_id| async move {
            let _permit = permit;
            app.archive_folder(&job_id, &source, &body, token)
                .instrument(span)
                .await
        })?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/gzip".to_owned()),
            (header::CONTENT_DISPOSITION, disposition(name)),
            (header::CACHE_CONTROL, "no-store".to_owned()),
            (header::HeaderName::from_static("x-job-id"), job_id),
        ],
        stall::download(chunks, outcome),
    )
        .into_response())
}

impl App {
    async fn archive_folder(
        &self,
        job_id: &str,
        source: &Source,
        body: &mpsc::Sender<std::io::Result<Bytes>>,
        token: auth::Session,
    ) -> anyhow::Result<Value> {
        self.append_log(job_id, &format!("Archiving {}.", source.label))?;
        let guard = match &source.lock {
            Some(resource) => Some(
                self.hold(resource)
                    .await
                    .map_err(|error| anyhow::anyhow!(error.1))?,
            ),
            None => None,
        };
        let mut child = self
            .command(&source.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Could not start Docker CLI")?;
        if let Some(helper) = &source.helper {
            self.helper_started(helper, &mut child).await;
        }
        drop(guard);
        // Held until tar has ended; when it closes early, tar stops where it runs.
        let stdin = child.stdin.take();
        let sent = send_archive(self, job_id, &mut child, body, token).await;
        drop(stdin);
        let sent = match sent {
            Ok(sent) => sent,
            Err(error) => {
                if let Some(helper) = &source.helper {
                    let _ = self.docker(&["rm", "--force", helper]).await;
                }
                return Err(error);
            }
        };
        self.append_log(job_id, &format!("Sent {sent} bytes."))?;
        Ok(json!({"path": source.path, "bytes": sent}))
    }
}

#[cfg(test)]
#[path = "folder_archive_tests.rs"]
mod tests;
