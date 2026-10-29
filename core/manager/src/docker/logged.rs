//! A Docker command a job runs with its output in the job's log, a whole line at a time where a
//! build or a compose setup is followed for its script's progress, within a time limit.
use crate::{App, script_progress::ScriptProgress};
use anyhow::{Context, Result, bail};
use std::{process::Stdio, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// How long a logged Docker command (a build, a pull, a script or a setup step) may run, unless
/// its caller gives a longer limit.
pub(crate) const LOGGED_LIMIT: Duration = Duration::from_secs(3600);

impl App {
    pub async fn docker_logged(&self, id: &str, args: &[String]) -> Result<()> {
        self.docker_logged_input(id, args, None).await
    }
    pub async fn docker_logged_input(
        &self,
        id: &str,
        args: &[String],
        input: Option<Vec<u8>>,
    ) -> Result<()> {
        self.run_logged(id, args, input, None, LOGGED_LIMIT).await
    }
    /// A compose setup step's `docker exec`: its output is logged a whole line at a time and
    /// followed for the script's progress, and it may run longer than the usual hour when the
    /// step's own timeout is longer.
    pub(crate) async fn docker_setup_logged(
        &self,
        id: &str,
        args: &[String],
        input: Vec<u8>,
        limit: Duration,
        progress: &mut ScriptProgress,
    ) -> Result<()> {
        self.run_logged(id, args, Some(input), Some(progress), limit)
            .await
    }
    /// A script build's `docker build`: its output is logged a whole line at a time and followed
    /// for the scripts' progress, and a line that begins a build stage starts it.
    pub(crate) async fn docker_build_logged(
        &self,
        id: &str,
        args: &[String],
        progress: &mut ScriptProgress,
    ) -> Result<()> {
        self.run_logged(id, args, None, Some(progress), LOGGED_LIMIT)
            .await
    }
    async fn run_logged(
        &self,
        id: &str,
        args: &[String],
        input: Option<Vec<u8>>,
        progress: Option<&mut ScriptProgress>,
        limit: Duration,
    ) -> Result<()> {
        let mut child = self
            .command(args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let out = child.stdout.take().unwrap();
        let err = child.stderr.take().unwrap();
        let stdin = child.stdin.take();
        let write = async move {
            // Scripts may exit before reading all of their input; the exit status is authoritative.
            if let (Some(mut stdin), Some(input)) = (stdin, input)
                && let Err(e) = async {
                    stdin.write_all(&input).await?;
                    stdin.shutdown().await
                }
                .await
            {
                tracing::debug!("Job input was not fully delivered: {e}");
            }
            anyhow::Ok(())
        };
        let read = async {
            let (mut out, mut err) = (out, err);
            let (mut a, mut b) = (false, false);
            let (mut dec_a, mut dec_b) = (
                crate::process::TextDecoder::default(),
                crate::process::TextDecoder::default(),
            );
            let (mut buf_a, mut buf_b) = ([0; 8192], [0; 8192]);
            let mut script =
                progress.map(|progress| (<[crate::stages::Lines; 2]>::default(), progress));
            let mut log = |stream: usize, text: String, end: bool| match &mut script {
                Some((lines, progress)) => {
                    let mut whole = lines[stream].push(&text);
                    if end {
                        whole.push_str(&lines[stream].finish());
                    }
                    self.log_script_output(id, &whole, progress)
                }
                None if text.is_empty() => Ok(()),
                None => self.append_log(id, &text),
            };
            while !a || !b {
                tokio::select! {
                    n = out.read(&mut buf_a), if !a => { let n=n?; a=n==0; let text=if a {dec_a.finish()} else {dec_a.push(&buf_a[..n])}; log(0,text,a)?; },
                    n = err.read(&mut buf_b), if !b => { let n=n?; b=n==0; let text=if b {dec_b.finish()} else {dec_b.push(&buf_b[..n])}; log(1,text,b)?; },
                }
            }
            let status = child.wait().await?;
            if !status.success() {
                self.append_log(id, &format!("Docker command exited with {status}."))?;
                bail!("Docker operation failed ({status}); check the job log");
            }
            anyhow::Ok(())
        };
        let result = tokio::time::timeout(limit, async {
            tokio::try_join!(write, read)?;
            anyhow::Ok(())
        })
        .await;
        if !matches!(result, Ok(Ok(()))) {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        result.with_context(|| match limit.as_secs() {
            3600 => "Image preparation exceeded the one-hour limit".to_owned(),
            seconds => format!("The operation exceeded its {}-minute limit", seconds / 60),
        })??;
        Ok(())
    }
}
