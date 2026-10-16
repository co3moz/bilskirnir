use crate::{
    App,
    desktop_input::{self, Action},
    error::{Error, Result},
    files, uploads,
};
use anyhow::{Context, bail};
use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::StatusCode,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use rmcp::schemars::{self, JsonSchema};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

/// Screenshot options shared by the HTTP route, the MCP tool and act-and-observe.
#[derive(Default, Deserialize, JsonSchema)]
pub struct Capture {
    /// Scale the image down to this width when the capture is wider (16–7680). Omit for full
    /// desktop resolution, where pixel coordinates match desktop_input.
    #[serde(default)]
    pub max_width: Option<u32>,
    /// Crop to "x,y,width,height" in desktop pixels before scaling.
    #[serde(default)]
    pub region: Option<String>,
    /// Capture one window by its X11 ID from list_windows instead of the whole desktop.
    #[serde(default)]
    pub window_id: Option<String>,
}
impl Capture {
    pub fn region(&self) -> anyhow::Result<Option<[u32; 4]>> {
        let Some(region) = &self.region else {
            return Ok(None);
        };
        let parts = region
            .split(',')
            .map(|part| part.trim().parse::<u32>())
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("region must be x,y,width,height in desktop pixels")?;
        let [x, y, width, height] = parts[..] else {
            bail!("region must be x,y,width,height in desktop pixels");
        };
        if width == 0 || height == 0 || [x, y, width, height].iter().any(|v| *v > 7680) {
            bail!("region must describe a non-empty area within the desktop");
        }
        Ok(Some([x, y, width, height]))
    }
    pub fn validate(&self) -> anyhow::Result<()> {
        if let Some(width) = self.max_width
            && !(16..=7680).contains(&width)
        {
            bail!("max_width must be 16–7680");
        }
        self.region()?;
        if let Some(window) = &self.window_id {
            crate::windows::window_id(window)?;
        }
        Ok(())
    }
}
pub struct Screenshot {
    pub jpeg: Vec<u8>,
    pub width: u16,
    pub height: u16,
    pub captured_at: u64,
}
impl Screenshot {
    pub fn summary(&self) -> Value {
        json!({"width": self.width, "height": self.height, "captured_at": self.captured_at, "bytes": self.jpeg.len()})
    }
}
/// Read the frame size from a JPEG start-of-frame segment.
pub fn jpeg_dimensions(bytes: &[u8]) -> Option<(u16, u16)> {
    if bytes.len() < 4 || bytes[0] != 0xff || bytes[1] != 0xd8 {
        return None;
    }
    let mut i = 2;
    while i + 4 <= bytes.len() {
        if bytes[i] != 0xff {
            return None;
        }
        let marker = bytes[i + 1];
        if marker == 0xff {
            i += 1;
            continue;
        }
        if marker == 0x01 || (0xd0..=0xd8).contains(&marker) {
            i += 2;
            continue;
        }
        if marker == 0xd9 || marker == 0xda {
            return None;
        }
        let length = usize::from(u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]));
        if (0xc0..=0xcf).contains(&marker) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
            if i + 9 > bytes.len() {
                return None;
            }
            let height = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]);
            let width = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]);
            return Some((width, height));
        }
        i += 2 + length;
    }
    None
}
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub async fn capture(app: &App, id: &str, options: &Capture) -> Result<Screenshot> {
    options.validate()?;
    let _permit = app
        .screenshots
        .acquire()
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let env = app.container(id)?;
    let info = app.owned(&env).await?;
    let region = options
        .region()?
        .map(|[x, y, width, height]| format!("{x},{y},{width},{height}"))
        .unwrap_or_default();
    let bytes = app
        .docker(&[
            "exec",
            "--user",
            "desktop",
            "--env",
            "DISPLAY=:0",
            info["Id"].as_str().context("Missing container ID")?,
            "/usr/local/bin/desktop-screenshot",
            &options.max_width.unwrap_or(0).to_string(),
            &region,
            options.window_id.as_deref().unwrap_or(""),
        ])
        .await?;
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return Err(anyhow::anyhow!("Screenshot not ready").into());
    }
    let (width, height) =
        jpeg_dimensions(&bytes).context("Screenshot dimensions could not be read")?;
    Ok(Screenshot {
        jpeg: bytes,
        width,
        height,
        captured_at: now_ms(),
    })
}
/// A screenshot saved inside the desktop, so the user can collect it through the dashboard.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveCapture {
    /// Absolute file path in Downloads or a writable mounted volume, written as the desktop user.
    pub path: String,
    /// Replace an existing file at path.
    #[serde(default)]
    pub replace: bool,
    #[serde(default)]
    pub capture: Capture,
}
impl SaveCapture {
    /// Split path into the directory and file name that the upload destination rules check.
    pub fn target(&self) -> anyhow::Result<(String, String)> {
        let target = self
            .path
            .strip_prefix('/')
            .and_then(|_| self.path.rsplit_once('/'))
            .filter(|(directory, name)| !directory.is_empty() && !name.is_empty());
        let Some((directory, name)) = target else {
            bail!("path must be an absolute file path in Downloads or a writable mounted volume");
        };
        Ok((directory.to_owned(), name.to_owned()))
    }
}
/// Capture a screenshot and store it inside the desktop under the upload destination rules.
/// The result describes the file by its path, size and dashboard download_url, so an agent can
/// hand the image to a person without relaying it through the conversation or the host.
pub async fn save_screenshot(app: &Arc<App>, id: &str, body: &SaveCapture) -> Result<Value> {
    let (destination, name) = body.target()?;
    let shot = capture(app, id, &body.capture).await?;
    let request = uploads::Upload {
        destination,
        name,
        size: shot.jpeg.len() as u64,
        replace: body.replace,
    };
    let saved = uploads::save_file(
        app,
        id.to_owned(),
        request,
        Body::from(shot.jpeg),
        std::future::pending(),
    )
    .await?
    .0;
    let path = saved["path"].as_str().unwrap_or_default().to_owned();
    let download_url = files::download_url(id, &path);
    Ok(json!({
        "path": path,
        "bytes": saved["bytes"],
        "width": shot.width,
        "height": shot.height,
        "captured_at": shot.captured_at,
        "download_url": download_url,
    }))
}
pub async fn save(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<SaveCapture>,
) -> Result<Json<Value>> {
    Ok(Json(save_screenshot(&app, &id, &body).await?))
}
/// Desktop input followed by a screenshot of the result.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Observe {
    pub actions: Vec<Action>,
    /// Milliseconds to wait after the input before capturing (0–10000).
    #[serde(default = "settle")]
    pub settle_ms: u64,
    #[serde(default)]
    pub capture: Capture,
}
fn settle() -> u64 {
    500
}
pub async fn act_and_observe(
    app: &Arc<App>,
    id: &str,
    body: &Observe,
) -> Result<(Value, Screenshot)> {
    if body.settle_ms > 10_000 {
        return Err(Error(
            StatusCode::BAD_REQUEST,
            "settle_ms must be at most 10000".into(),
        ));
    }
    body.capture.validate()?;
    let input = desktop_input::perform(app, id, &body.actions).await?;
    if body.settle_ms > 0 {
        tokio::time::sleep(Duration::from_millis(body.settle_ms)).await;
    }
    let shot = capture(app, id, &body.capture).await?;
    Ok((input, shot))
}
pub async fn act(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<Observe>,
) -> Result<Json<Value>> {
    let (input, shot) = act_and_observe(&app, &id, &body).await?;
    let mut screenshot = shot.summary();
    screenshot["jpeg_base64"] = json!(STANDARD.encode(&shot.jpeg));
    Ok(Json(json!({"input": input, "screenshot": screenshot})))
}
