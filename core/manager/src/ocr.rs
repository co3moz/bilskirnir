//! Text on a desktop's screen, read with Tesseract OCR: `find_text` and the text condition of
//! `wait_for_condition`. Games, Unity players and many Electron applications expose no
//! accessibility tree to `inspect_ui`, so their screen is all an agent has of them.
//!
//! The screen is the screenshot at desktop resolution, so the boxes are desktop pixels that
//! `desktop_input` takes as they are. It is read in gray and enlarged up to twice first, since
//! interface text is smaller than the print Tesseract was trained on, and the boxes are scaled
//! back. Tesseract reads it twice at once, in sparse text mode (11), which finds the scattered
//! labels of an interface, and in its automatic page mode (3), which reads the words near a
//! window's edge that sparse mode merges into the edge; the words of both are merged.
//!
//! Tesseract runs in the manager, never in the desktop, so it reads every desktop image, older
//! and imported ones too; a manager without it, such as a build run outside its image, answers
//! that it is missing.
mod words;

use crate::{
    App,
    capture::{self, Capture, Gray},
    error::{Error, Result},
    process::{Overflow, read_limited},
};
use anyhow::{anyhow, bail};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rmcp::schemars::{self, JsonSchema};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{process::Stdio, sync::Arc, time::Duration};
use tokio::{io::AsyncWriteExt, process::Command, sync::Semaphore};
pub(crate) use words::Match;

/// The program, found on the manager's `PATH`.
const PROGRAM: &str = "tesseract";
/// The longest one reading may take.
const TIMEOUT: Duration = Duration::from_secs(30);
/// The page segmentation modes a reading runs Tesseract in: sparse text, and automatic page.
const MODES: [&str; 2] = ["11", "3"];
/// The most TSV one run may print.
const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;
/// The pixels a capture is enlarged to at most: a 1920x1080 desktop twice over.
const TARGET_PIXELS: f64 = 3840.0 * 2160.0;
/// The words one reading keeps, more than a dense desktop holds.
const WORDS: usize = 3000;
/// The lines or words, and the matches, an answer lists.
const ENTRIES: usize = 500;
const MATCHES: usize = 50;
/// The longest text to look for.
pub(crate) const TEXT_LIMIT: usize = 256;
/// Readings at once. Each keeps a core busy for every mode, and a wait for text reads again at
/// every check.
static RUNNING: Semaphore = Semaphore::const_new(2);

#[derive(Clone, Copy, Debug, Default, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    #[default]
    Line,
    Word,
}

/// What to read from the screen and what to look for in it.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FindText {
    /// Text to look for, at most 256 bytes. It matches within one line, ignoring case unless
    /// case_sensitive, with any run of spaces matching one; matches lists every place with its box
    /// and center in desktop pixels.
    #[serde(default)]
    pub text: Option<String>,
    /// Read only x,y,width,height in desktop pixels; defaults to the whole desktop.
    #[serde(default)]
    pub region: Option<String>,
    /// "line" (default) lists lines of words with their boxes; "word" lists every word instead.
    #[serde(default)]
    pub level: Level,
    /// Leave out words Tesseract is less sure of, 0 to 100 (default 50); lower it for faint or
    /// stylized text.
    #[serde(default = "min_confidence")]
    pub min_confidence: u8,
    /// Match text with its case.
    #[serde(default)]
    pub case_sensitive: bool,
}
fn min_confidence() -> u8 {
    50
}
impl FindText {
    pub fn validate(&self) -> anyhow::Result<()> {
        if let Some(text) = &self.text {
            check_text(text, "text")?;
        }
        if self.min_confidence > 100 {
            bail!("min_confidence must be 0 to 100");
        }
        region(self.region.as_deref()).validate()
    }
}

/// A text to look for: some characters other than spaces, at most [`TEXT_LIMIT`] bytes.
pub(crate) fn check_text(text: &str, name: &str) -> anyhow::Result<()> {
    if text.trim().is_empty() || text.len() > TEXT_LIMIT {
        bail!("{name} must be 1 to {TEXT_LIMIT} bytes, not only spaces");
    }
    Ok(())
}

/// A capture of the region, or of the whole desktop, at desktop resolution.
fn region(region: Option<&str>) -> Capture {
    Capture {
        max_width: None,
        region: region.map(str::to_owned),
        window_id: None,
    }
}

/// What the screen, or a region of it, reads as.
pub(crate) struct Reading {
    /// The size of what was read, the region's when there is one.
    pub width: u16,
    pub height: u16,
    pub captured_at: u64,
    /// Top to bottom, of at most [`WORDS`] words.
    pub lines: Vec<words::Line>,
    /// Whether Tesseract read more words than were kept.
    pub truncated: bool,
}

/// How many times a capture of this size is enlarged before Tesseract reads it.
fn enlargement(width: usize, height: usize) -> f64 {
    (TARGET_PIXELS / (width * height).max(1) as f64)
        .sqrt()
        .clamp(1.0, 2.0)
}

/// Read the text in a region of the desktop, or all of it.
pub(crate) async fn read(
    app: &App,
    id: &str,
    area: Option<&str>,
    min_confidence: u8,
) -> Result<Reading> {
    let options = region(area);
    let origin = options
        .region()?
        .map(|[x, y, _, _]| (x, y))
        .unwrap_or_default();
    let shot = capture::capture(app, id, &options).await?;
    let (width, height, captured_at) = (shot.width, shot.height, shot.captured_at);
    let _permit = RUNNING.acquire().await.map_err(|e| anyhow!(e))?;
    // Decoding and enlarging a desktop's pixels is work for a blocking thread.
    let (image, factor) = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let gray = Gray::from_jpeg(&shot.jpeg)?;
        let factor = enlargement(gray.width, gray.height);
        Ok((gray.enlarged(factor).pgm(), factor))
    })
    .await
    .map_err(|e| anyhow!(e))??;
    let [sparse, page] = MODES;
    let readings = tokio::try_join!(
        tesseract(PROGRAM, image.clone(), sparse, TIMEOUT),
        tesseract(PROGRAM, image, page, TIMEOUT),
    )?;
    // Merging the readings and making lines compare every word with the others.
    let (lines, truncated) = tokio::task::spawn_blocking(move || {
        let mut truncated = false;
        let readings = <[String; 2]>::from(readings).map(|tsv| {
            let mut found = words::parse(
                &tsv,
                factor,
                origin,
                (width, height),
                f32::from(min_confidence),
            );
            truncated |= found.len() > WORDS;
            found.truncate(WORDS);
            found
        });
        let mut words = words::merge(readings);
        truncated |= words.len() > WORDS;
        words.truncate(WORDS);
        (words::lines(&words), truncated)
    })
    .await
    .map_err(|e| anyhow!(e))?;
    Ok(Reading {
        width,
        height,
        captured_at,
        lines,
        truncated,
    })
}

/// Where `text` is on the screen, or in a region of it, as the text condition of a wait reads it.
pub(crate) async fn locate(
    app: &App,
    id: &str,
    area: Option<&str>,
    text: &str,
) -> Result<Vec<Match>> {
    let reading = read(app, id, area, min_confidence()).await?;
    Ok(words::find(&reading.lines, text, false))
}

pub async fn find_text(app: &App, id: &str, body: &FindText) -> Result<Value> {
    body.validate()?;
    let reading = read(app, id, body.region.as_deref(), body.min_confidence).await?;
    let (key, entries): (&str, Vec<Value>) = match body.level {
        Level::Line => (
            "lines",
            reading.lines.iter().map(words::Line::json).collect(),
        ),
        // In the order of the lines, so a row's words come left to right.
        Level::Word => (
            "words",
            reading
                .lines
                .iter()
                .flat_map(|line| &line.words)
                .map(words::Word::json)
                .collect(),
        ),
    };
    let mut result = json!({
        "width": reading.width,
        "height": reading.height,
        "region": body.region,
        "captured_at": reading.captured_at,
        "truncated": reading.truncated || entries.len() > ENTRIES,
    });
    result[key] = json!(entries.into_iter().take(ENTRIES).collect::<Vec<_>>());
    if let Some(text) = &body.text {
        let matches = words::find(&reading.lines, text, body.case_sensitive);
        result["text"] = json!(text);
        result["found"] = json!(!matches.is_empty());
        result["matches"] = json!(
            matches
                .iter()
                .take(MATCHES)
                .map(Match::json)
                .collect::<Vec<_>>()
        );
    }
    Ok(result)
}

pub async fn find(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<FindText>,
) -> Result<Json<Value>> {
    Ok(Json(find_text(&app, &id, &body).await?))
}

/// The answer of a manager without Tesseract.
fn missing() -> Error {
    Error(
        StatusCode::NOT_IMPLEMENTED,
        "Reading text needs Tesseract OCR with its English data (the Debian packages tesseract-ocr and tesseract-ocr-eng) where the manager runs; the published manager image has both".into(),
    )
}

/// Run Tesseract on a PGM image in a page segmentation `mode` and return its TSV: English, one
/// thread, at most `timeout` and [`OUTPUT_LIMIT`].
async fn tesseract(program: &str, image: Vec<u8>, mode: &str, timeout: Duration) -> Result<String> {
    let spawned = Command::new(program)
        .args(["stdin", "stdout", "--psm", mode, "-l", "eng", "tsv"])
        // Readings at once share the cores instead of each taking all of them.
        .env("OMP_THREAD_LIMIT", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Err(missing()),
        Err(error) => return Err(anyhow!("Tesseract could not start: {error}").into()),
    };
    let (Some(mut stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Err(anyhow!("Tesseract's pipes could not be opened").into());
    };
    let run = async {
        tokio::try_join!(
            async move {
                // A Tesseract that fails before it reads everything closes the pipe; its exit
                // status and message say more than the broken pipe would.
                if let Err(error) = async {
                    stdin.write_all(&image).await?;
                    stdin.shutdown().await
                }
                .await
                {
                    tracing::debug!("Tesseract did not read the whole image: {error}");
                }
                Ok::<_, anyhow::Error>(())
            },
            read_limited(stdout, OUTPUT_LIMIT, Overflow::Reject),
            read_limited(stderr, 64 * 1024, Overflow::Tail),
            async { Ok::<_, anyhow::Error>(child.wait().await?) },
        )
    };
    let (_, stdout, stderr, status) = match tokio::time::timeout(timeout, run).await {
        Ok(Ok(done)) => done,
        Ok(Err(error)) => {
            let _ = child.kill().await;
            return Err(Error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Tesseract failed: {error:#}"),
            ));
        }
        Err(_) => {
            let _ = child.kill().await;
            return Err(Error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!(
                    "Reading the text took longer than {} seconds; read a smaller region",
                    timeout.as_secs_f32()
                ),
            ));
        }
    };
    if !status.success() {
        let message = String::from_utf8_lossy(&stderr);
        let message: Vec<&str> = message
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        let tail = message[message.len().saturating_sub(3)..].join(" ");
        return Err(Error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Tesseract failed ({status}): {tail}"),
        ));
    }
    Ok(String::from_utf8_lossy(&stdout).into_owned())
}

#[cfg(test)]
#[path = "ocr_tests.rs"]
mod tests;
