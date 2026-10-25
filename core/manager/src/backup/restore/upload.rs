//! A restore's upload as the backup's entries, and where a part's data goes: `docker load`
//! through the filter an image import uses, or tar unpacking a new volume.
use crate::{
    App,
    archives::{count, import_progress, interrupted, loaded_image, tar::Filter},
    auth,
    backup::{
        export::hex,
        format::{CHUNK, GZIP_STEP, Item, Name, Reader, SMALL_LIMIT, parse_name},
        manifest::PartEnd,
    },
    process::{self, Overflow},
    volumes::VolumeWriter,
};
use anyhow::{Context, anyhow, bail};
use axum::body::{Body, BodyDataStream, Bytes};
use blake2::{Blake2s256, Digest};
use futures_util::StreamExt;
use std::{
    collections::VecDeque,
    pin::Pin,
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::{
    io::AsyncWriteExt,
    process::{Child, ChildStdin},
    task::JoinHandle,
};

/// How often the bytes received so far are written to the job.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);
/// `docker load` registers the layers after the archive has arrived; a large image takes minutes.
const LOAD_LIMIT: Duration = Duration::from_secs(3600);

/// The upload as the archive's entries, read one step at a time so a highly compressed upload
/// cannot fill memory.
pub(super) struct Input<'a> {
    app: &'a App,
    pub job_id: &'a str,
    stream: BodyDataStream,
    reader: Option<Reader>,
    pending: Bytes,
    items: VecDeque<Item>,
    received: u64,
    size: Option<u64>,
    revoked: Pin<Box<dyn Future<Output = ()> + Send + 'a>>,
    reported: Instant,
}

impl<'a> Input<'a> {
    /// The upload `body` of `size` bytes, or of unknown size, which `token` keeps going.
    pub(super) fn new(
        app: &'a App,
        job_id: &'a str,
        size: Option<u64>,
        body: Body,
        token: auth::Session,
    ) -> Self {
        Self {
            app,
            job_id,
            stream: body.into_data_stream(),
            reader: Some(Reader::new(app.config.archive_limit)),
            pending: Bytes::new(),
            items: VecDeque::new(),
            received: 0,
            size,
            revoked: Box::pin(app.session_revoked(token)),
            reported: Instant::now(),
        }
    }

    /// What the archive holds next, or `None` once it has ended whole.
    pub(super) async fn next(&mut self) -> anyhow::Result<Option<Item>> {
        loop {
            if let Some(item) = self.items.pop_front() {
                return Ok(Some(item));
            }
            let Some(reader) = self.reader.as_mut() else {
                return Ok(None);
            };
            let mut found = Vec::new();
            if !self.pending.is_empty() {
                let step = self.pending.split_to(self.pending.len().min(GZIP_STEP));
                reader.push(&step, &mut found)?;
                self.items.extend(found);
                continue;
            }
            let chunk = tokio::select! {
                chunk = self.stream.next() => chunk,
                _ = &mut self.revoked => bail!("Session expired. Sign in again."),
            };
            match chunk {
                Some(chunk) => {
                    let chunk = chunk.map_err(interrupted)?;
                    self.received += chunk.len() as u64;
                    count(self.received, self.size, self.app.config.archive_limit)?;
                    self.pending = chunk;
                    if self.reported.elapsed() >= PROGRESS_INTERVAL {
                        self.app.set_progress(
                            self.job_id,
                            Some(import_progress(self.received, self.size)),
                        )?;
                        self.reported = Instant::now();
                    }
                }
                None => {
                    if let Some(size) = self.size
                        && self.received != size
                    {
                        bail!("The upload ended after {} of {size} bytes", self.received);
                    }
                    let reader = self.reader.take().context("The backup has ended")?;
                    reader.finish(&mut found)?;
                    self.items.extend(found);
                }
            }
        }
    }

    /// The next entry's name and length.
    async fn begin(&mut self) -> anyhow::Result<(Name, u64)> {
        match self.next().await? {
            Some(Item::Begin { name, size }) => Ok((parse_name(&name)?, size)),
            Some(_) => bail!("The backup is damaged"),
            None => bail!("The backup ended before its last entry; the file is incomplete"),
        }
    }

    /// The rest of the current entry, of `size` bytes and at most `limit`.
    async fn whole(&mut self, size: u64, limit: u64, what: &str) -> anyhow::Result<Vec<u8>> {
        if size > limit {
            bail!("The backup's {what} is larger than {} KiB", limit >> 10);
        }
        let mut data = Vec::with_capacity(size as usize);
        loop {
            match self.next().await? {
                Some(Item::Data(more)) => data.extend_from_slice(&more),
                Some(Item::Finish) => return Ok(data),
                _ => bail!("The backup ended in its {what}; the file is incomplete"),
            }
        }
    }

    /// The entry `expected`, whole.
    pub(super) async fn expect(
        &mut self,
        expected: Name,
        limit: u64,
        what: &str,
    ) -> anyhow::Result<Vec<u8>> {
        let (name, size) = self.begin().await?;
        if name != expected {
            bail!("The backup's entries are out of order: its {what} is missing");
        }
        self.whole(size, limit, what).await
    }

    /// Hand part `index`'s data to `sink` in order, then check its length and digest against the
    /// part's end entry. Returns the part's length.
    pub(super) async fn part(&mut self, index: usize, mut sink: Sink<'_>) -> anyhow::Result<u64> {
        let mut hasher = Blake2s256::new();
        let (mut total, mut next_chunk) = (0u64, 0u64);
        loop {
            let (name, size) = self.begin().await?;
            match name {
                Name::Chunk { part, chunk } if part == index && chunk == next_chunk => {
                    if size > CHUNK as u64 {
                        bail!("The backup is damaged: a chunk of part {index} is too large");
                    }
                    loop {
                        match self.next().await? {
                            Some(Item::Data(data)) => {
                                hasher.update(&data);
                                total += data.len() as u64;
                                sink.write(&data).await?;
                            }
                            Some(Item::Finish) => break,
                            _ => bail!("The backup ended in part {index}; the file is incomplete"),
                        }
                    }
                    next_chunk += 1;
                }
                Name::PartEnd { part } if part == index => {
                    let end: PartEnd =
                        serde_json::from_slice(&self.whole(size, SMALL_LIMIT, "part end").await?)
                            .context("The backup is damaged: a part's end cannot be read")?;
                    if end.bytes != total || end.blake2s != hex(&hasher.finalize()) {
                        bail!(
                            "Part {index} of the backup is damaged: its length or digest does not match"
                        );
                    }
                    return Ok(total);
                }
                _ => bail!("The backup's entries are out of order in part {index}"),
            }
        }
    }
}

/// Where a part's data goes.
pub(super) enum Sink<'a> {
    Load(&'a mut Load),
    Volume(&'a mut VolumeWriter),
}

impl Sink<'_> {
    async fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
        match self {
            Sink::Load(load) => load.write(data).await,
            Sink::Volume(volume) => volume.write(data).await,
        }
    }
}

/// `docker load` reading one part through the filter an image import uses, which takes out every
/// name the stream carries and holds back what Docker registers an image from until the stream
/// has been checked. Dropped before it finished, the CLI is killed before its input closes.
pub(super) struct Load {
    child: Child,
    stdin: Option<ChildStdin>,
    filter: Filter,
    output: JoinHandle<anyhow::Result<Vec<u8>>>,
    errors: JoinHandle<anyhow::Result<Vec<u8>>>,
    /// Set once Docker stopped reading; its own error output says why.
    stopped: bool,
}

impl Load {
    pub(super) fn start(app: &App) -> anyhow::Result<Self> {
        let mut child = app
            .command(&["load".into(), "--quiet".into()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Could not start Docker CLI")?;
        let output = tokio::spawn(process::read_limited(
            child.stdout.take().context("Missing docker load output")?,
            65_536,
            Overflow::Tail,
        ));
        let errors = tokio::spawn(process::read_limited(
            child.stderr.take().context("Missing docker load errors")?,
            8192,
            Overflow::Tail,
        ));
        Ok(Self {
            stdin: child.stdin.take(),
            child,
            filter: Filter::new(app.config.archive_limit),
            output,
            errors,
            stopped: false,
        })
    }

    async fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
        self.filter.push(data)?;
        let output = self.filter.take_output();
        let stdin = self.stdin.as_mut().context("docker load input is closed")?;
        if !output.is_empty() && stdin.write_all(&output).await.is_err() {
            self.stopped = true;
            bail!("docker load stopped reading the image");
        }
        Ok(())
    }

    /// Close the input after the checked closing entries and return the loaded image's ID.
    pub(super) async fn finish(mut self) -> anyhow::Result<String> {
        let (tail, _) = self.filter.finish()?;
        let mut stdin = self.stdin.take().context("docker load input is closed")?;
        if stdin.write_all(&tail).await.is_err() || stdin.shutdown().await.is_err() {
            return Err(Self::docker_error(self.errors).await);
        }
        drop(stdin);
        let status = tokio::time::timeout(LOAD_LIMIT, self.child.wait())
            .await
            .context("docker load exceeded the one-hour limit")??;
        let output = self.output.await??;
        let errors = self.errors.await??;
        if !status.success() {
            bail!("Docker: {}", String::from_utf8_lossy(&errors).trim());
        }
        loaded_image(&String::from_utf8_lossy(&output))
    }

    /// End a load whose restore failed with `error`, and return what explains it best.
    pub(super) async fn fail(mut self, error: anyhow::Error) -> anyhow::Error {
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
        if self.stopped {
            Self::docker_error(self.errors).await
        } else {
            error
        }
    }

    async fn docker_error(errors: JoinHandle<anyhow::Result<Vec<u8>>>) -> anyhow::Error {
        let errors = match errors.await {
            Ok(Ok(errors)) => errors,
            _ => Vec::new(),
        };
        anyhow!("Docker: {}", String::from_utf8_lossy(&errors).trim())
    }
}
