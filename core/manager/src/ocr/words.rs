//! Tesseract's words, the lines they make on the screen and where a text is among them.
//!
//! A reading merges the words of two of Tesseract's page segmentation modes, keeping the surer
//! word where two overlap. Neither keeps the lines of an interface: sparse text mode puts nearly
//! every label in a block of its own and breaks a line where a space is wide, and the page mode
//! joins labels a column apart. Lines are therefore made here from where the words are: a word
//! continues a line when it sits on the same row and starts within one and a half line heights
//! of its end. That joins the words of a phrase or a terminal line and keeps labels a column
//! apart, such as the buttons of a panel, separate.
use serde_json::{Value, json};

/// How far, in line heights, the next word of a line may start after the line's end.
const GAP: f64 = 1.5;

/// A word Tesseract read, its box in desktop pixels and how sure it is, 0 to 100.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Word {
    pub text: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub confidence: f32,
}

impl Word {
    fn right(&self) -> u32 {
        self.x + self.width
    }
    fn bottom(&self) -> u32 {
        self.y + self.height
    }
    pub fn json(&self) -> Value {
        json!({
            "text": self.text,
            "x": self.x,
            "y": self.y,
            "width": self.width,
            "height": self.height,
            "confidence": self.confidence.round() as u32,
        })
    }
}

/// Words on one row, left to right.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Line {
    pub words: Vec<Word>,
}

impl Line {
    pub fn text(&self) -> String {
        join(&self.words)
    }
    fn top(&self) -> u32 {
        self.words.iter().map(|w| w.y).min().unwrap_or_default()
    }
    fn bottom(&self) -> u32 {
        self.words
            .iter()
            .map(Word::bottom)
            .max()
            .unwrap_or_default()
    }
    fn right(&self) -> u32 {
        self.words.last().map(Word::right).unwrap_or_default()
    }
    pub fn json(&self) -> Value {
        let mut value = bounds(&self.words, &self.text());
        let total: f32 = self.words.iter().map(|w| w.confidence).sum();
        value["confidence"] = json!((total / self.words.len().max(1) as f32).round() as u32);
        value
    }
}

/// Where a text was found: the words it covers, as one box.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Match {
    pub words: Vec<Word>,
}

impl Match {
    pub fn json(&self) -> Value {
        let mut value = bounds(&self.words, &join(&self.words));
        let width = value["width"].as_u64().unwrap_or_default();
        let height = value["height"].as_u64().unwrap_or_default();
        value["center_x"] = json!(value["x"].as_u64().unwrap_or_default() + width / 2);
        value["center_y"] = json!(value["y"].as_u64().unwrap_or_default() + height / 2);
        let least = self
            .words
            .iter()
            .map(|w| w.confidence)
            .fold(100.0, f32::min);
        value["confidence"] = json!(least.round() as u32);
        value
    }
}

fn join(words: &[Word]) -> String {
    words
        .iter()
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The text and the box around some words.
fn bounds(words: &[Word], text: &str) -> Value {
    let x = words.iter().map(|w| w.x).min().unwrap_or_default();
    let y = words.iter().map(|w| w.y).min().unwrap_or_default();
    let right = words.iter().map(Word::right).max().unwrap_or_default();
    let bottom = words.iter().map(Word::bottom).max().unwrap_or_default();
    json!({"text": text, "x": x, "y": y, "width": right - x, "height": bottom - y})
}

/// The words of Tesseract's TSV output that it is at least `min_confidence` sure of, their boxes
/// taken back from an image enlarged `factor` times to desktop pixels, where the image's top left
/// corner is `origin` and its size `size`.
pub(crate) fn parse(
    tsv: &str,
    factor: f64,
    origin: (u32, u32),
    size: (u16, u16),
    min_confidence: f32,
) -> Vec<Word> {
    let (width, height) = (u32::from(size.0).max(1), u32::from(size.1).max(1));
    let mut words = Vec::new();
    for row in tsv.lines() {
        // level page block paragraph line word left top width height confidence text
        let columns: Vec<&str> = row.split('\t').collect();
        let [level, _, _, _, _, _, left, top, w, h, confidence, text] = columns[..] else {
            continue;
        };
        let text = text.trim();
        let (Ok(5), Ok(left), Ok(top), Ok(w), Ok(h), Ok(confidence)) = (
            level.parse::<u8>(),
            left.parse::<f64>(),
            top.parse::<f64>(),
            w.parse::<f64>(),
            h.parse::<f64>(),
            confidence.parse::<f32>(),
        ) else {
            continue;
        };
        if text.is_empty() || confidence < min_confidence.max(0.0) {
            continue;
        }
        let scale = |value: f64, limit: u32, round: fn(f64) -> f64| {
            (round(value / factor).max(0.0) as u32).min(limit)
        };
        let (x0, y0) = (
            scale(left, width - 1, f64::floor),
            scale(top, height - 1, f64::floor),
        );
        let (x1, y1) = (
            scale(left + w, width, f64::ceil).max(x0 + 1),
            scale(top + h, height, f64::ceil).max(y0 + 1),
        );
        words.push(Word {
            text: text.to_owned(),
            x: origin.0 + x0,
            y: origin.1 + y0,
            width: x1 - x0,
            height: y1 - y0,
            confidence: confidence.min(100.0),
        });
    }
    words
}

/// The words of several readings of one image, top to bottom: where words overlap by more than
/// half of the smaller one, they are one word read differently, and the surer reading is kept.
pub(crate) fn merge<const N: usize>(readings: [Vec<Word>; N]) -> Vec<Word> {
    let mut all: Vec<Word> = readings.into_iter().flatten().collect();
    all.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
    let mut kept: Vec<Word> = Vec::with_capacity(all.len());
    for word in all {
        let area = |w: &Word| u64::from(w.width) * u64::from(w.height);
        let overlapping = kept.iter().any(|other| {
            let width = word
                .right()
                .min(other.right())
                .saturating_sub(word.x.max(other.x));
            let height = word
                .bottom()
                .min(other.bottom())
                .saturating_sub(word.y.max(other.y));
            u64::from(width) * u64::from(height) * 2 > area(&word).min(area(other))
        });
        if !overlapping {
            kept.push(word);
        }
    }
    kept.sort_by_key(|w| (w.y, w.x));
    kept
}

/// The words in lines, top to bottom and left to right.
pub(crate) fn lines(words: &[Word]) -> Vec<Line> {
    let mut sorted: Vec<&Word> = words.iter().collect();
    sorted.sort_by_key(|w| (w.x, w.y));
    let mut lines: Vec<Line> = Vec::new();
    for word in sorted {
        let continued = lines
            .iter_mut()
            .filter_map(|line| {
                let (top, bottom) = (line.top(), line.bottom());
                let tall = f64::from((bottom - top).max(word.height));
                let overlap = bottom.min(word.bottom()).saturating_sub(top.max(word.y));
                let shorter = (bottom - top).min(word.height);
                let gap = f64::from(word.x) - f64::from(line.right());
                (f64::from(overlap) * 2.0 >= f64::from(shorter)
                    && gap >= -tall / 2.0
                    && gap <= tall * GAP)
                    .then_some((gap, line))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        match continued {
            Some((_, line)) => line.words.push(word.clone()),
            None => lines.push(Line {
                words: vec![word.clone()],
            }),
        }
    }
    // Top to bottom, and left to right among the lines of one row, which start within half a
    // line's height of each other.
    lines.sort_by_key(|line| (line.top(), line.words[0].x));
    let mut ordered = Vec::with_capacity(lines.len());
    let mut row: Vec<Line> = Vec::new();
    for line in lines {
        if let Some(first) = row.first()
            && line.top() > first.top() + (first.bottom() - first.top()) / 2
        {
            row.sort_by_key(|line| line.words[0].x);
            ordered.append(&mut row);
        }
        row.push(line);
    }
    row.sort_by_key(|line| line.words[0].x);
    ordered.append(&mut row);
    ordered
}

/// Every place `text` is read within one line: case is ignored unless `case_sensitive`, and a run
/// of spaces matches the space between two words.
pub(crate) fn find(lines: &[Line], text: &str, case_sensitive: bool) -> Vec<Match> {
    let fold = |s: &str| {
        if case_sensitive {
            s.to_owned()
        } else {
            s.to_lowercase()
        }
    };
    let needle = fold(&text.split_whitespace().collect::<Vec<_>>().join(" "));
    if needle.is_empty() {
        return Vec::new();
    }
    let mut found = Vec::new();
    for line in lines {
        // The line's words joined by spaces, with where each word lies in it.
        let mut haystack = String::new();
        let mut spans = Vec::with_capacity(line.words.len());
        for word in &line.words {
            if !haystack.is_empty() {
                haystack.push(' ');
            }
            let start = haystack.len();
            haystack.push_str(&fold(&word.text));
            spans.push(start..haystack.len());
        }
        for (start, matched) in haystack.match_indices(&needle) {
            let end = start + matched.len();
            let words: Vec<Word> = line
                .words
                .iter()
                .zip(&spans)
                .filter(|(_, span)| span.start < end && span.end > start)
                .map(|(word, _)| word.clone())
                .collect();
            if !words.is_empty() {
                found.push(Match { words });
            }
        }
    }
    found
}

#[cfg(test)]
#[path = "words_tests.rs"]
mod tests;
