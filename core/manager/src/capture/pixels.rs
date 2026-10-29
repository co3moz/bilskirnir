//! The pixels of a screenshot, for what the manager reads from the screen itself: its text, which
//! Tesseract reads from a grayscale image, and how much of it changed while a wait ran.
use crate::error::Fault;
use anyhow::{Context, bail};
use axum::http::StatusCode;

/// The most pixels a screenshot may decode to, an 8K desktop's.
const MAX_PIXELS: usize = 7680 * 4320;

/// A grayscale image, one byte a pixel, row after row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Gray {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

impl Gray {
    /// Decode a screenshot's JPEG. Its size is read from the header first, so an image larger
    /// than any desktop is refused before a pixel is decoded. Bytes that are no desktop's JPEG
    /// are a failure of the desktop's screenshot helper, which answers 502.
    pub fn from_jpeg(jpeg: &[u8]) -> anyhow::Result<Self> {
        Self::decode(jpeg)
            .map_err(|error| Fault::error(StatusCode::BAD_GATEWAY, format!("{error:#}")))
    }
    fn decode(jpeg: &[u8]) -> anyhow::Result<Self> {
        let mut decoder = jpeg_decoder::Decoder::new(jpeg);
        decoder.set_max_decoding_buffer_size(MAX_PIXELS * 3);
        decoder
            .read_info()
            .context("The screenshot could not be decoded")?;
        let info = decoder
            .info()
            .context("The screenshot could not be decoded")?;
        let (width, height) = (usize::from(info.width), usize::from(info.height));
        if width == 0 || height == 0 || width * height > MAX_PIXELS {
            bail!("The screenshot is {width}x{height}, which no desktop is");
        }
        let data = decoder
            .decode()
            .context("The screenshot could not be decoded")?;
        let pixels: Vec<u8> = match info.pixel_format {
            jpeg_decoder::PixelFormat::L8 => data,
            jpeg_decoder::PixelFormat::RGB24 => data
                .as_chunks::<3>()
                .0
                .iter()
                .map(|&[r, g, b]| luma(r, g, b))
                .collect(),
            other => bail!("The screenshot has pixels in {other:?}, not RGB"),
        };
        if pixels.len() != width * height {
            bail!("The screenshot's pixels do not fill its size");
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// The image enlarged `factor` times (1 or more) with bilinear interpolation, so that small
    /// interface text reaches the size Tesseract reads best.
    pub fn enlarged(&self, factor: f64) -> Self {
        if factor <= 1.0 {
            return self.clone();
        }
        let width = ((self.width as f64 * factor).round() as usize).max(1);
        let height = ((self.height as f64 * factor).round() as usize).max(1);
        let (sx, sy) = (
            self.width as f64 / width as f64,
            self.height as f64 / height as f64,
        );
        // Where each output column falls between two source columns, worked out once.
        let columns: Vec<(usize, usize, f64)> = (0..width)
            .map(|x| between((x as f64 + 0.5) * sx - 0.5, self.width))
            .collect();
        let mut pixels = Vec::with_capacity(width * height);
        for y in 0..height {
            let (top, bottom, fy) = between((y as f64 + 0.5) * sy - 0.5, self.height);
            let (top, bottom) = (
                &self.pixels[top * self.width..][..self.width],
                &self.pixels[bottom * self.width..][..self.width],
            );
            for &(left, right, fx) in &columns {
                let upper = f64::from(top[left]) * (1.0 - fx) + f64::from(top[right]) * fx;
                let lower = f64::from(bottom[left]) * (1.0 - fx) + f64::from(bottom[right]) * fx;
                pixels.push((upper * (1.0 - fy) + lower * fy).round() as u8);
            }
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    /// The image as a binary PGM, which Tesseract reads from its standard input.
    pub fn pgm(&self) -> Vec<u8> {
        let mut out = format!("P5\n{} {}\n255\n", self.width, self.height).into_bytes();
        out.extend_from_slice(&self.pixels);
        out
    }

    /// The percentage of pixels that differ from `other` by more than `threshold` levels; all of
    /// them when the two are not the same size, as a window that resized has changed.
    pub fn changed_percent(&self, other: &Gray, threshold: u8) -> f64 {
        if self.width != other.width || self.height != other.height || self.pixels.is_empty() {
            return 100.0;
        }
        let changed = self
            .pixels
            .iter()
            .zip(&other.pixels)
            .filter(|(a, b)| a.abs_diff(**b) > threshold)
            .count();
        changed as f64 * 100.0 / self.pixels.len() as f64
    }
}

/// A pixel's luma from its red, green and blue, weighted as ITU-R BT.601 weighs them.
fn luma(r: u8, g: u8, b: u8) -> u8 {
    ((77 * u32::from(r) + 150 * u32::from(g) + 29 * u32::from(b) + 128) >> 8) as u8
}

/// The two source pixels a position lies between and how far it is from the first.
fn between(position: f64, size: usize) -> (usize, usize, f64) {
    let position = position.clamp(0.0, (size - 1) as f64);
    let first = position.floor() as usize;
    (first, (first + 1).min(size - 1), position - first as f64)
}

#[cfg(test)]
#[path = "pixels_tests.rs"]
mod tests;
