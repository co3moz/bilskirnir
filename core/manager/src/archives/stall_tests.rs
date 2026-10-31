//! A transfer ends once its client takes or sends nothing for [`STALL`], however long the
//! transfer itself has run, and at once when its session ends; a large chunk is handed over in
//! pieces, each with its own wait; a download's body ends with its job's error without the job
//! waiting for room to send it. Paused time stands in for the wait.
use super::*;
use tokio::time::Instant;

fn chunk() -> Bytes {
    Bytes::from_static(b"chunk")
}

#[tokio::test(start_paused = true)]
async fn a_download_nobody_reads_stops_after_the_stall() {
    let (body, _unread) = mpsc::channel(1);
    body.try_send(Ok(chunk())).unwrap();
    let started = Instant::now();
    let error = send(&body, chunk(), &mut std::future::pending())
        .await
        .unwrap_err();
    assert_eq!(started.elapsed(), STALL);
    assert_eq!(
        error.to_string(),
        "The download stopped: the client took nothing for 5 minutes"
    );
}

#[tokio::test(start_paused = true)]
async fn a_slow_client_keeps_its_download_past_the_stall() {
    let (body, mut chunks) = mpsc::channel(1);
    let reader = tokio::spawn(async move {
        let mut taken = 0;
        loop {
            tokio::time::sleep(STALL - Duration::from_secs(1)).await;
            match chunks.recv().await {
                Some(_) => taken += 1,
                None => return taken,
            }
        }
    });
    let started = Instant::now();
    for _ in 0..5 {
        send(&body, chunk(), &mut std::future::pending())
            .await
            .unwrap();
    }
    drop(body);
    assert_eq!(reader.await.unwrap(), 5);
    assert!(started.elapsed() > 3 * STALL);
}

#[tokio::test(start_paused = true)]
async fn a_large_chunk_goes_in_pieces_each_with_its_own_wait() {
    let (body, mut chunks) = mpsc::channel::<io::Result<Bytes>>(1);
    // As hyper takes a frame only once it has sent most of the one before, a client this slow
    // takes a piece just inside the limit, and would take a whole backup chunk far beyond it.
    let reader = tokio::spawn(async move {
        let mut pieces = Vec::new();
        loop {
            tokio::time::sleep(STALL - Duration::from_secs(1)).await;
            match chunks.recv().await {
                Some(piece) => pieces.push(piece.unwrap().len()),
                None => return pieces,
            }
        }
    });
    let started = Instant::now();
    // As large as a backup's chunk.
    let chunk = Bytes::from(vec![7; 4 << 20]);
    send(&body, chunk, &mut std::future::pending())
        .await
        .unwrap();
    drop(body);
    let pieces = reader.await.unwrap();
    assert_eq!(pieces, vec![PIECE; (4 << 20) / PIECE]);
    assert!(started.elapsed() > 10 * STALL);
    // Nothing to hand over sends nothing.
    let (body, mut chunks) = mpsc::channel(1);
    send(&body, Bytes::new(), &mut std::future::pending())
        .await
        .unwrap();
    drop(body);
    assert!(chunks.recv().await.is_none());
}

#[tokio::test(start_paused = true)]
async fn an_ended_session_or_a_gone_client_ends_a_download_at_once() {
    let (body, chunks) = mpsc::channel(1);
    body.try_send(Ok(chunk())).unwrap();
    let started = Instant::now();
    let error = send(&body, chunk(), &mut std::future::ready(()))
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), EXPIRED);
    drop(chunks);
    let error = send(&body, chunk(), &mut std::future::pending())
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), CANCELLED);
    assert_eq!(started.elapsed(), Duration::ZERO);
}

#[tokio::test(start_paused = true)]
async fn an_upload_that_sends_nothing_stops_after_the_stall() {
    let mut arrived = futures_util::stream::iter([1, 2]);
    let mut session = std::future::pending();
    assert_eq!(receive(&mut arrived, &mut session).await.unwrap(), Some(1));
    assert_eq!(receive(&mut arrived, &mut session).await.unwrap(), Some(2));
    assert_eq!(receive(&mut arrived, &mut session).await.unwrap(), None);

    let mut silent = futures_util::stream::pending::<u8>();
    let started = Instant::now();
    let error = receive(&mut silent, &mut session).await.unwrap_err();
    assert_eq!(started.elapsed(), STALL);
    assert_eq!(
        error.to_string(),
        "The upload stopped: nothing arrived for 5 minutes"
    );
    // The upload answers as one the request should not have sent, with that message.
    let answer = crate::error::Error::from(error);
    assert_eq!(answer.0, axum::http::StatusCode::BAD_REQUEST);

    let error = receive(&mut silent, &mut std::future::ready(()))
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), EXPIRED);
    assert_eq!(started.elapsed(), STALL);

    // An image import waits longer for the image's first bytes.
    let error = receive_within(&mut silent, &mut session, START)
        .await
        .unwrap_err();
    assert_eq!(started.elapsed(), STALL + START);
    assert_eq!(
        error.to_string(),
        "The upload stopped: nothing arrived for 60 minutes"
    );
}

#[tokio::test(start_paused = true)]
async fn a_stall_is_logged_once_at_warn_in_the_span_it_runs_in() {
    let (written, _logging) = crate::test_support::capture();
    let span = tracing::info_span!("request", path = %"/api/transfers/<token>");
    let (body, _unread) = mpsc::channel(1);
    body.try_send(Ok(chunk())).unwrap();
    let _ =
        tracing::Instrument::instrument(send(&body, chunk(), &mut std::future::pending()), span)
            .await;
    let log = written.text();
    let lines = log
        .lines()
        .filter(|line| line.contains("took nothing"))
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 1, "{log:?}");
    assert!(lines[0].contains(" WARN "), "{log:?}");
    assert!(
        lines[0].contains("request{path=/api/transfers/<token>}"),
        "{log:?}"
    );
}

/// What a download's body hands its client: the data, and the error that ends it, if any.
async fn delivered(body: Body) -> (Vec<u8>, Option<String>) {
    let mut stream = body.into_data_stream();
    let mut data = Vec::new();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(chunk) => data.extend_from_slice(&chunk),
            Err(error) => return (data, Some(error.to_string())),
        }
    }
    (data, None)
}

#[tokio::test]
async fn a_download_ends_cleanly_only_when_its_job_succeeded() {
    for (outcome, expected) in [
        (Some(Ok(serde_json::json!({}))), None),
        (
            Some(Err("Docker: no space left".to_owned())),
            Some("Docker: no space left"),
        ),
        // A job that ended without an outcome, as one whose manager stopped.
        (None, Some("The transfer ended unexpectedly")),
    ] {
        // The client stopped reading: the job's last chunk fills the channel, and it ends
        // without waiting for room to report how.
        let (sender, chunks) = mpsc::channel(1);
        sender.try_send(Ok(chunk())).unwrap();
        drop(sender);
        let (done, received) = tokio::sync::oneshot::channel();
        match outcome {
            Some(outcome) => done.send(outcome).unwrap(),
            None => drop(done),
        }
        let (data, error) = delivered(download(chunks, received)).await;
        assert_eq!(data, b"chunk");
        assert_eq!(error.as_deref(), expected);
    }
}
