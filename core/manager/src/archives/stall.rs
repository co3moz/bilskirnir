//! How long a transfer waits for its client. hyper ends a response body only once its connection
//! closes, and the manager sets no timeout on a connection, so a browser that pauses a download,
//! or any client that stops reading or sending while its connection stays open, would keep the
//! transfer's Docker process and its transfer slot until the manager restarts. Two such clients
//! would leave every other archive transfer answered with 429.
//!
//! Every piece a transfer hands its client, and every chunk it waits for, may take at most
//! [`STALL`]. A transfer that waited that long ends as a cancelled one does: its child process
//! stops, its job fails and its slot is free again. A transfer link and an installation without
//! sign-in have no session that could end the transfer, so this is the one thing that ends theirs.
use crate::jobs;
use anyhow::anyhow;
use axum::body::{Body, Bytes};
use futures_util::{Stream, StreamExt};
use std::{io, time::Duration};
use tokio::sync::mpsc;

/// The longest a transfer waits for its client to take or send the next piece. A download is
/// handed over in pieces of at most [`PIECE`] bytes, which hyper takes one at a time as it sends
/// them, so only a client slower than about 1 KB/s runs into it while it still reads.
pub(crate) const STALL: Duration = Duration::from_secs(300);
/// The longest an image import waits for the image's own first bytes. `docker save` on Docker's
/// classic image store prepares the whole image before it writes any, so an archive piped from
/// it, or from Thoric's export after the export's first entry, can arrive after a long silence.
pub(crate) const START: Duration = Duration::from_secs(3600);
/// The most of a download handed over at once, so the time a client may take for one piece does
/// not grow with the chunk a transfer made, such as a backup's chunks of a few MiB.
const PIECE: usize = 256 * 1024;
/// What a download fails with once its client is gone.
pub(crate) const CANCELLED: &str = "The download was cancelled before it finished";
const EXPIRED: &str = "Session expired. Sign in again.";

/// Hand `chunk` to the download `body`, in pieces of at most [`PIECE`] bytes. Each waits at most
/// [`STALL`] for the client to take what was handed over before, and it ends at once when
/// `revoked` resolves.
pub(crate) async fn send(
    body: &mpsc::Sender<io::Result<Bytes>>,
    mut chunk: Bytes,
    revoked: &mut (impl Future<Output = ()> + Unpin),
) -> anyhow::Result<()> {
    while !chunk.is_empty() {
        let piece = chunk.split_to(chunk.len().min(PIECE));
        tokio::select! {
            sent = tokio::time::timeout(STALL, body.send(Ok(piece))) => match sent {
                Ok(sent) => sent.map_err(|_| anyhow!(CANCELLED))?,
                Err(_) => {
                    return Err(stalled(format!(
                        "The download stopped: the client took nothing for {}",
                        minutes(STALL)
                    )));
                }
            },
            () = &mut *revoked => return Err(anyhow!(EXPIRED)),
        }
    }
    Ok(())
}

/// The next chunk of an upload `stream`, or `None` once it has ended. It waits at most [`STALL`]
/// for the chunk, and ends at once when `revoked` resolves.
pub(crate) async fn receive<S: Stream + Unpin>(
    stream: &mut S,
    revoked: &mut (impl Future<Output = ()> + Unpin),
) -> anyhow::Result<Option<S::Item>> {
    receive_within(stream, revoked, STALL).await
}

/// [`receive`] that waits at most `limit`, such as [`START`] for an image's first bytes.
pub(crate) async fn receive_within<S: Stream + Unpin>(
    stream: &mut S,
    revoked: &mut (impl Future<Output = ()> + Unpin),
    limit: Duration,
) -> anyhow::Result<Option<S::Item>> {
    tokio::select! {
        next = tokio::time::timeout(limit, stream.next()) => next.map_err(|_| stalled(format!(
            "The upload stopped: nothing arrived for {}",
            minutes(limit)
        ))),
        () = revoked => Err(anyhow!(EXPIRED)),
    }
}

/// The body of a download whose chunks a job hands to the other end of `chunks`. It ends cleanly
/// only when the job succeeded and otherwise with the job's error, so a download that stopped
/// early never looks like a complete file. The job needs no room in the channel to say so, which a
/// client that stopped reading would never give it.
pub(crate) fn download(chunks: mpsc::Receiver<io::Result<Bytes>>, outcome: jobs::Outcome) -> Body {
    let stream = futures_util::stream::unfold(Some((chunks, outcome)), |state| async move {
        let (mut chunks, outcome) = state?;
        if let Some(chunk) = chunks.recv().await {
            return Some((chunk, Some((chunks, outcome))));
        }
        let error = match outcome.await {
            Ok(Ok(_)) => return None,
            Ok(Err(message)) => message,
            Err(_) => "The transfer ended unexpectedly".to_owned(),
        };
        Some((Err(io::Error::other(error)), None))
    });
    Body::from_stream(stream)
}

/// A stall, logged once at warn as it ends the transfer. The transfer runs in the span of the
/// request that started it, so the line names that request, as `request_log` gives a handler's.
fn stalled(message: String) -> anyhow::Error {
    tracing::warn!("{message}");
    anyhow!(message)
}

fn minutes(limit: Duration) -> String {
    format!("{} minutes", limit.as_secs() / 60)
}

#[cfg(test)]
#[path = "stall_tests.rs"]
mod tests;
