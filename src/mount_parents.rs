//! The folders of the desktop's home above a volume's mount point. Docker makes the missing ones,
//! owned by root, when the container starts, and already when anything is copied into or out of
//! the created container, as its machine ID is. A volume at `~/.config/chrome-agent` in an image
//! without `~/.config` then leaves the desktop user a configuration folder it cannot write, and
//! the XFCE session, which starts long before a compose's setup could hand the folder over, stops
//! at "Unable to load a failsafe session" and the desktop never gets ready.
//!
//! So before a container starts, every folder on the way from the home to a mount point that root
//! owns is given to the desktop user, keeping its mode and time. `docker cp` reads and writes them,
//! since a container that has not started runs nothing. The way ends at a symbolic link, a file or
//! another volume's folder. Folders outside the home stay as Docker makes them, since root may own
//! those on purpose, and `setup_paths.py` hands them over after the start where it can tell.
use crate::{
    App,
    archives::tar,
    model::*,
    process::{Overflow, capture_input, read_limited},
};
use anyhow::{Context, Result, bail};
use std::{collections::HashMap, process::Stdio, time::Duration};
use tokio::io::AsyncReadExt;

const HOME: &str = "/home/desktop";
/// The desktop user and its group.
const DESKTOP: u32 = 1001;
/// Enough of an archive for a folder's own entry after an extension header. The rest holds
/// everything in the folder, the volumes under it too, and is never read.
const HEAD: u64 = 64 * 1024;
const TIMEOUT: Duration = Duration::from_secs(120);

/// A folder's own entry, the first of the archive `docker cp` makes of it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Entry {
    pub directory: bool,
    pub uid: i64,
    pub mode: u32,
    pub mtime: u64,
}

/// For each mount point in the home, the folders above it there, relative to the home and the
/// nearest to the home first. A folder that another volume is mounted at, or one inside it, ends
/// the way: it holds that volume's files.
pub(crate) fn ways(mounts: &[Mount]) -> Vec<Vec<String>> {
    let mounted = |folder: &str| {
        let path = format!("{HOME}/{folder}");
        mounts
            .iter()
            .any(|mount| path == mount.target || path.starts_with(&format!("{}/", mount.target)))
    };
    mounts
        .iter()
        .filter_map(|mount| mount.target.strip_prefix(HOME)?.strip_prefix('/'))
        .map(|relative| {
            let parts = relative.split('/').collect::<Vec<_>>();
            (1..parts.len())
                .map(|depth| parts[..depth].join("/"))
                .take_while(|folder| !mounted(folder))
                .collect::<Vec<_>>()
        })
        .filter(|way| !way.is_empty())
        .collect()
}

/// The first entry of an archive, after the extension headers that describe it, or none when the
/// archive ends before one.
pub(crate) fn first_entry(mut archive: &[u8]) -> Result<Option<Entry>> {
    while let Some(header) = archive.get(..tar::BLOCK) {
        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        if &header[257..262] != b"ustar" {
            bail!("Docker's archive is not a USTAR, PAX or GNU tar archive");
        }
        if matches!(header[156], b'x' | b'g' | b'L' | b'K') {
            let size = tar::size(&header[124..136])?;
            let skip = usize::try_from(tar::BLOCK as u64 + size + tar::padding(size))?;
            archive = archive.get(skip..).unwrap_or_default();
            continue;
        }
        return Ok(Some(Entry {
            directory: header[156] == b'5',
            uid: tar::numeric(&header[108..116])?,
            mode: (tar::numeric(&header[100..108])? & 0o7777) as u32,
            mtime: u64::try_from(tar::numeric(&header[136..148])?).unwrap_or(0),
        }));
    }
    Ok(None)
}

/// The archive that gives the folders, relative to the home, to the desktop user. A folder whose
/// name does not fit a header stays as it is.
pub(crate) fn hand_over_archive(folders: &[(String, Entry)]) -> Vec<u8> {
    let mut archive = Vec::new();
    for (folder, entry) in folders {
        match tar::directory(folder, entry.mode, DESKTOP, entry.mtime) {
            Some(header) => archive.extend_from_slice(&header),
            None => tracing::warn!("{HOME}/{folder} stays root's: its name is too long"),
        }
    }
    // Two empty blocks end an archive.
    archive.resize(archive.len() + 2 * tar::BLOCK, 0);
    archive
}

impl App {
    /// Give the desktop user the folders root owns on the way from the home to each of the
    /// container's mount points, before the container starts.
    pub(crate) async fn hand_over_mount_parents(&self, env: &Container) -> Result<()> {
        let mut read = HashMap::<String, Option<Entry>>::new();
        let mut owned_by_root = Vec::new();
        for way in ways(&env.mounts()) {
            for folder in way {
                let entry = match read.get(&folder) {
                    Some(entry) => entry.clone(),
                    None => {
                        let entry = self.folder_entry(&env.container, &folder).await?;
                        if let Some(found) = entry.as_ref().filter(|e| e.directory && e.uid == 0) {
                            owned_by_root.push((folder.clone(), found.clone()));
                        }
                        read.insert(folder, entry.clone());
                        entry
                    }
                };
                if !entry.is_some_and(|entry| entry.directory) {
                    break;
                }
            }
        }
        if owned_by_root.is_empty() {
            return Ok(());
        }
        // Docker gives the folders the owner the archive names. Not with `--archive`: that gives
        // them the image's user, and images Bilskirnir builds are left with root.
        let args = ["cp".into(), "-".into(), format!("{}:{HOME}", env.container)];
        let out = capture_input(
            self.command(&args),
            TIMEOUT,
            65_536,
            Overflow::Tail,
            Some(hand_over_archive(&owned_by_root)),
        )
        .await?;
        if !out.status.success() {
            bail!(
                "Docker could not give the desktop user the folders above its volumes: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(())
    }

    /// A folder of the home as `docker cp` archives it, read no further than its own entry.
    async fn folder_entry(&self, container: &str, folder: &str) -> Result<Option<Entry>> {
        let args = [
            "cp".into(),
            format!("{container}:{HOME}/{folder}"),
            "-".into(),
        ];
        let mut child = self
            .command(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Could not start Docker CLI")?;
        let stdout = child.stdout.take().context("Docker CLI has no output")?;
        let stderr = child
            .stderr
            .take()
            .context("Docker CLI has no error output")?;
        let mut head = Vec::new();
        let mut start = stdout.take(HEAD);
        let read = tokio::time::timeout(TIMEOUT, start.read_to_end(&mut head)).await;
        if !head.is_empty() {
            let _ = child.kill().await;
            return first_entry(&head)
                .with_context(|| format!("Docker's archive of {HOME}/{folder} is unreadable"));
        }
        read.context("Docker operation timed out")??;
        let finished = async {
            let stderr = read_limited(stderr, 65_536, Overflow::Tail).await?;
            anyhow::Ok((child.wait().await?, stderr))
        };
        let (status, stderr) = tokio::time::timeout(TIMEOUT, finished)
            .await
            .context("Docker operation timed out")??;
        if !status.success() {
            bail!(
                "Docker could not read {HOME}/{folder}: {}",
                String::from_utf8_lossy(&stderr).trim()
            );
        }
        Ok(None)
    }
}

#[cfg(test)]
#[path = "mount_parents_tests.rs"]
mod tests;
