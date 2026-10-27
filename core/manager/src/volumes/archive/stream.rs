//! A volume's files as a plain tar stream for an installation backup, and a new volume unpacked
//! from one in a restore, through the same helpers an export and an import use: the reader as
//! root on a read-only mount with only `DAC_READ_SEARCH`, the writer as the desktop user into a
//! volume it created a moment ago, keeping the files' modes as a copy does.
use super::{EXTRACT_LIMIT, Helper, Stopped, Unpacking};
use crate::{
    App,
    error::{Error, Result},
    process::{self, Overflow},
    volumes::{Access, MOUNT},
};
use anyhow::{Context, anyhow, bail};
use axum::http::StatusCode;
use tokio::{io::AsyncWriteExt, process::ChildStdout, task::JoinHandle};

/// How tar unpacks a restored volume: as the desktop user, keeping the modes it was saved with.
/// A setuid bit grants nothing beyond that user, who owns every file.
const RESTORED: &[&str] = &["--preserve-permissions"];

/// tar writing a volume's files, read from `stdout`.
pub(crate) struct VolumeReader {
    helper: Helper,
    pub stdout: ChildStdout,
    errors: JoinHandle<anyhow::Result<Vec<u8>>>,
}

impl VolumeReader {
    /// Wait for tar once its output has been read. It ends with 1 when a file changed while it was
    /// read, as one in use can; the archive is complete and holds the file as it was read, so that
    /// is returned as a warning.
    pub(crate) async fn finish(mut self, app: &App) -> anyhow::Result<Option<String>> {
        let status = self.helper.child.wait().await?;
        let errors = String::from_utf8_lossy(&self.errors.await??)
            .trim()
            .to_owned();
        match status.code() {
            Some(0) => Ok(None),
            Some(1) => Ok(Some(errors)),
            _ => {
                let _ = app.docker(&["rm", "--force", &self.helper.name]).await;
                bail!("tar: {errors}")
            }
        }
    }

    /// Stop a reader whose output is no longer wanted.
    pub(crate) async fn abandon(self, app: &App) {
        self.helper.abandon(app).await;
    }
}

/// tar unpacking a restored volume's files from what `write` hands it.
pub(crate) struct VolumeWriter {
    unpacking: Unpacking,
    /// Set once tar no longer reads what it is handed; its own error output says why.
    stopped: bool,
}

impl VolumeWriter {
    pub(crate) async fn write(&mut self, data: &[u8]) -> anyhow::Result<()> {
        if self.unpacking.stdin.write_all(data).await.is_err() {
            self.stopped = true;
            return Err(anyhow!(Stopped));
        }
        Ok(())
    }

    /// End tar's input and wait until it has written everything.
    pub(crate) async fn finish(self, app: &App) -> anyhow::Result<()> {
        let Unpacking {
            mut helper,
            mut stdin,
            errors,
        } = self.unpacking;
        let closed = stdin.shutdown().await;
        drop(stdin);
        let status = match tokio::time::timeout(EXTRACT_LIMIT, helper.child.wait()).await {
            Ok(status) => status?,
            Err(_) => {
                helper.abandon(app).await;
                bail!("Unpacking the volume exceeded the one-hour limit");
            }
        };
        let errors = String::from_utf8_lossy(&errors.await??).trim().to_owned();
        if closed.is_err() || !status.success() {
            bail!("tar: {errors}");
        }
        Ok(())
    }

    /// End a writer whose restore failed with `error`, and return what explains it best: tar's
    /// own message when tar stopped reading, otherwise `error`.
    pub(crate) async fn fail(self, app: &App, error: anyhow::Error) -> anyhow::Error {
        let Unpacking {
            mut helper,
            stdin,
            errors,
        } = self.unpacking;
        drop(stdin);
        if !self.stopped {
            helper.abandon(app).await;
            return error;
        }
        let status = helper.child.wait().await;
        let errors = match errors.await {
            Ok(Ok(errors)) => String::from_utf8_lossy(&errors).trim().to_owned(),
            _ => String::new(),
        };
        if status.is_err() || errors.is_empty() {
            helper.abandon(app).await;
            return anyhow!("tar stopped reading the volume's files");
        }
        anyhow!("tar: {errors}")
    }
}

impl App {
    /// Start tar reading the volume `name` for a backup: one a container mounts, also when
    /// `VOLUME_FILTER` leaves it out, or one Thoric created, never the manager's own. The
    /// volume's lock is held until Docker has the helper, as for an export.
    pub(crate) async fn read_volume_files(&self, name: &str) -> anyhow::Result<VolumeReader> {
        let _guard = self
            .hold(&format!("volume:{name}"))
            .await
            .map_err(|error| anyhow!(error.1))?;
        let volume = self
            .volume_named(name)
            .await
            .map_err(|error| anyhow!(error.1))?;
        // `docker run` would create a volume that is missing.
        self.inspect("volume", name)
            .await
            .with_context(|| format!("The volume {name} is no longer there"))?;
        let mut helper = Helper::spawn(
            self,
            &volume,
            Access::Archive,
            &[
                "tar",
                "--create",
                "--numeric-owner",
                "--file",
                "-",
                "--directory",
                MOUNT,
                ".",
            ],
        )?;
        self.helper_started(&helper.name, &mut helper.child).await;
        let stdout = helper.child.stdout.take().context("Missing tar output")?;
        let errors = tokio::spawn(process::read_limited(
            helper.child.stderr.take().context("Missing tar errors")?,
            8192,
            Overflow::Tail,
        ));
        Ok(VolumeReader {
            helper,
            stdout,
            errors,
        })
    }

    /// Create the volume `name` with `labels` beside the manager's own, and start tar unpacking
    /// into it. The caller holds the volume's lock; a volume of that name refuses the restore.
    pub(crate) async fn write_volume_files(
        &self,
        name: &str,
        labels: &[String],
    ) -> Result<VolumeWriter> {
        let volume = self.volume_named(name).await?;
        if self.optional_inspect("volume", name).await?.is_some() {
            return Err(Error(
                StatusCode::CONFLICT,
                format!("A volume named {name} already exists"),
            ));
        }
        self.make_volume(name, labels).await?;
        let unpacking = match self.start_unpack(&volume, "/", RESTORED).await {
            Ok(unpacking) => unpacking,
            Err(error) => {
                let _ = self.docker(&["volume", "rm", name]).await;
                return Err(error);
            }
        };
        Ok(VolumeWriter {
            unpacking,
            stopped: false,
        })
    }
}
