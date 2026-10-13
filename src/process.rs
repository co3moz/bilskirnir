use anyhow::{Context, Result, bail};
use std::{
    process::{Output, Stdio},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
};

#[derive(Clone, Copy)]
pub enum Overflow {
    Reject,
    Tail,
}

pub(crate) async fn read_limited(
    mut read: impl AsyncRead + Unpin,
    limit: usize,
    overflow: Overflow,
) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut chunk = [0; 8192];
    loop {
        let n = read.read(&mut chunk).await?;
        if n == 0 {
            return Ok(result);
        }
        if result.len() + n > limit {
            match overflow {
                Overflow::Reject => bail!("Docker output exceeded the {limit}-byte limit"),
                Overflow::Tail => {
                    let discard = (result.len() + n - limit).min(result.len());
                    result.drain(..discard);
                }
            }
        }
        result.extend_from_slice(&chunk[..n]);
        if result.len() > limit {
            result.drain(..result.len() - limit);
        }
    }
}
pub async fn capture(
    command: Command,
    timeout: Duration,
    limit: usize,
    overflow: Overflow,
) -> Result<Output> {
    capture_input(command, timeout, limit, overflow, None).await
}
pub async fn capture_input(
    mut command: Command,
    timeout: Duration,
    limit: usize,
    overflow: Overflow,
    input: Option<Vec<u8>>,
) -> Result<Output> {
    let mut child = command
        .kill_on_drop(true)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Could not start Docker CLI")?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let stdin = child.stdin.take();
    let result = tokio::time::timeout(timeout, async {
        let (stdout, stderr, status, _) = tokio::try_join!(
            read_limited(stdout, limit, overflow),
            read_limited(stderr, limit, overflow),
            async { Ok::<_, anyhow::Error>(child.wait().await?) },
            async move {
                if let (Some(mut stdin), Some(input)) = (stdin, input) {
                    stdin.write_all(&input).await?;
                    stdin.shutdown().await?;
                }
                Ok::<_, anyhow::Error>(())
            }
        )?;
        Ok::<_, anyhow::Error>(Output {
            status,
            stdout,
            stderr,
        })
    })
    .await;
    match result {
        Ok(Ok(output)) => Ok(output),
        failure => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            match failure {
                Ok(Err(e)) => Err(e),
                Err(_) => bail!("Docker operation timed out"),
                _ => unreachable!(),
            }
        }
    }
}

// Preserve split UTF-8 sequences; non-text script output must not abort a successful build.
#[derive(Default)]
pub struct TextDecoder {
    pending: Vec<u8>,
}
impl TextDecoder {
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut text = String::new();
        let mut offset = 0;
        while offset < self.pending.len() {
            match std::str::from_utf8(&self.pending[offset..]) {
                Ok(s) => {
                    text.push_str(s);
                    offset = self.pending.len();
                }
                Err(e) => {
                    let end = offset + e.valid_up_to();
                    text.push_str(&String::from_utf8_lossy(&self.pending[offset..end]));
                    offset = end;
                    match e.error_len() {
                        Some(n) => {
                            text.push('\u{fffd}');
                            offset += n;
                        }
                        None => break,
                    }
                }
            }
        }
        self.pending.drain(..offset);
        text
    }
    pub fn finish(&mut self) -> String {
        let tail = String::from_utf8_lossy(&self.pending).into_owned();
        self.pending.clear();
        tail
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utf8_split_and_invalid_bytes_do_not_abort_logs() {
        let mut decoder = TextDecoder::default();
        assert_eq!(decoder.push(&[b'a', 0xc4]), "a");
        assert_eq!(decoder.push(&[0xb0, 0xff, b'b']), "\u{0130}\u{fffd}b");
        assert_eq!(decoder.push(&[0xe2]), "");
        assert_eq!(decoder.finish(), "\u{fffd}");
    }
    #[tokio::test]
    async fn arbitrary_no_newline_output_is_bounded() {
        let bytes = vec![b'x'; 100_000];
        assert!(
            read_limited(&bytes[..], 1024, Overflow::Reject)
                .await
                .is_err()
        );
        let tail = read_limited(&bytes[..], 1024, Overflow::Tail)
            .await
            .unwrap();
        assert_eq!(tail.len(), 1024);
    }
    #[tokio::test]
    async fn command_timeout_includes_waiting_for_exit() {
        #[cfg(windows)]
        let command = {
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"]);
            c
        };
        #[cfg(not(windows))]
        let command = {
            let mut c = Command::new("sleep");
            c.arg("30");
            c
        };
        let start = std::time::Instant::now();
        assert!(
            capture(command, Duration::from_millis(100), 1024, Overflow::Reject)
                .await
                .unwrap_err()
                .to_string()
                .contains("timed out")
        );
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}
