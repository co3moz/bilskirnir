//! Image icons: a small picture that makes a saved image easy to find in a list. The interface
//! scales a chosen picture down before sending it, so the manager only checks that what arrives
//! is a PNG of at most 256 by 256 pixels and keeps its bytes on the image record. Lists carry a
//! version of the icon instead of the picture, and `GET /api/images/{id}/icon` returns it.
use crate::{
    App,
    error::{Error, Result},
    model::ImageRecord,
};
use anyhow::bail;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use serde_json::Value;
use std::sync::Arc;

/// The largest width and height of an icon, in pixels.
pub const ICON_SIZE: u32 = 256;
/// An uncompressed 256 by 256 RGBA PNG still fits.
const ICON_LIMIT: usize = 300 * 1024;
const DATA_URL: &str = "data:image/png;base64,";
const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
/// The header of the chunk every PNG starts with: its 13-byte length and its type.
const HEADER_CHUNK: &[u8] = b"\0\0\0\x0dIHDR";

/// A PNG icon. JSON carries it as a `data:image/png;base64,` URL, so an exported build
/// configuration holds its icon as text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Icon(Vec<u8>);

impl Icon {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.0.len() > ICON_LIMIT {
            bail!("An icon must be at most 300 KiB");
        }
        let Some((width, height)) = dimensions(&self.0) else {
            bail!("An icon must be a PNG image");
        };
        if !(1..=ICON_SIZE).contains(&width) || !(1..=ICON_SIZE).contains(&height) {
            bail!(
                "An icon must be at most {ICON_SIZE} by {ICON_SIZE} pixels, not {width} by {height}"
            );
        }
        Ok(())
    }

    /// Changes with the picture, so a list can name an icon URL that browsers keep.
    pub fn version(&self) -> String {
        // FNV-1a: a cache key, not a checksum anything relies on.
        let hash = self.0.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
        format!("{hash:016x}")
    }
}

/// The start of a PNG of this size, which is all an icon's checks read.
#[cfg(test)]
pub(crate) fn sample(width: u32, height: u32) -> Icon {
    let mut png = [SIGNATURE, HEADER_CHUNK].concat();
    png.extend(width.to_be_bytes());
    png.extend(height.to_be_bytes());
    png.extend([8, 6, 0, 0, 0]);
    Icon(png)
}

/// The width and height a PNG's header chunk gives, or `None` for anything else.
fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(..8)? != SIGNATURE || bytes.get(8..16)? != HEADER_CHUNK {
        return None;
    }
    let width = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?);
    let height = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?);
    Some((width, height))
}

impl Serialize for Icon {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.collect_str(&format_args!("{DATA_URL}{}", STANDARD.encode(&self.0)))
    }
}

impl<'de> Deserialize<'de> for Icon {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let data = text
            .strip_prefix(DATA_URL)
            .ok_or_else(|| D::Error::custom("An icon must be a data:image/png;base64, URL"))?;
        // Refused before decoding, so an oversized request costs no second copy.
        if data.len() > ICON_LIMIT.div_ceil(3) * 4 {
            return Err(D::Error::custom("An icon must be at most 300 KiB"));
        }
        STANDARD
            .decode(data)
            .map(Icon)
            .map_err(|_| D::Error::custom("The icon is not valid base64"))
    }
}

/// Reads an icon that was saved earlier, by this release or another one. A picture this release
/// cannot use reads as no icon, so a record or an archive keeps everything else it holds.
pub(crate) fn lenient<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Icon>, D::Error> {
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(value
        .and_then(|value| serde_json::from_value::<Icon>(value).ok())
        .filter(|icon| icon.validate().is_ok()))
}

/// The icon a snapshot of a container started from `image_id` keeps: that of the saved image
/// the container came from.
pub(crate) fn inherited(records: &[ImageRecord], image_id: &str) -> Option<Icon> {
    records
        .iter()
        .filter(|record| record.image_id == image_id)
        .find_map(|record| record.icon.clone())
}

#[derive(Deserialize)]
pub struct IconQuery {
    /// The version a list named; the picture behind it never changes.
    #[serde(default)]
    v: Option<String>,
}

/// A saved image's icon as a PNG.
pub async fn get(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Query(query): Query<IconQuery>,
    headers: HeaderMap,
) -> Result<Response> {
    let record: ImageRecord = app.db.get("image", &id)?;
    let icon = record
        .icon
        .ok_or_else(|| Error(StatusCode::NOT_FOUND, "This image has no icon".into()))?;
    let version = icon.version();
    let tag = format!("\"{version}\"");
    let cache = if query.v.as_deref() == Some(version.as_str()) {
        "private, max-age=31536000, immutable"
    } else {
        "private, no-cache"
    };
    let unchanged = headers
        .get(header::IF_NONE_MATCH)
        .is_some_and(|value| value.as_bytes() == tag.as_bytes());
    let headers = [
        (header::CACHE_CONTROL, cache.to_owned()),
        (header::ETAG, tag),
    ];
    Ok(if unchanged {
        (StatusCode::NOT_MODIFIED, headers).into_response()
    } else {
        let png = [(header::CONTENT_TYPE, "image/png")];
        (headers, png, icon.0).into_response()
    })
}

#[derive(Deserialize)]
pub struct SetIcon {
    pub icon: Icon,
}

pub async fn set(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    Json(body): Json<SetIcon>,
) -> Result<Json<Value>> {
    body.icon.validate()?;
    replace(&app, &id, Some(body.icon)).await
}

pub async fn remove(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    replace(&app, &id, None).await
}

async fn replace(app: &App, id: &str, icon: Option<Icon>) -> Result<Json<Value>> {
    let lock = app.resource_lock(&format!("image:{id}"));
    let _guard = lock.lock().await;
    let mut record: ImageRecord = app.db.get("image", id)?;
    record.icon = icon;
    app.db.put("image", id, &record)?;
    Ok(Json(crate::api::image_summary(&record)))
}

#[cfg(test)]
#[path = "icons_tests.rs"]
mod tests;
