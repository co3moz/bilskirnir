//! What an image's disk space is made of. Docker's history lists the steps that made an image
//! and the bytes each one wrote; the image's root filesystem lists its layers. Each step that
//! wrote a layer is matched to that layer, so steps are compared across images by the layers
//! below them rather than by their history, which a rebuild changes even when the layers it
//! produces are the same. A step belongs to the listed image with the fewest layers that has it,
//! the image it came from, and a layer several images share is stored once.
//!
//! Docker never changes the layers or the history of an image ID, so both are read once per ID.
use crate::{
    App,
    error::{Error, Result, missing},
};
use anyhow::Context;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

/// BuildKit's comment on every step it records; it says nothing about the step.
const BUILDKIT: &str = "buildkit.dockerfile.v0";
/// A step's instruction is cut at this many bytes; a long RUN line still names its command.
const INSTRUCTION_LIMIT: usize = 4096;

/// The images whose layers and history were read, by Docker image ID.
#[derive(Default)]
pub struct Cache(Mutex<HashMap<String, Arc<Image>>>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum StepKind {
    /// A command ran; the files it wrote are the layer.
    Run,
    /// Files copied from the build context or another build stage.
    Copy,
    /// Files added from an archive or a URL, as an operating system's base layer is.
    Add,
    /// A setting such as an environment variable or a label, which writes no files.
    Config,
    /// A container's files saved with `docker commit`: a snapshot, or a settings change that
    /// carried a container's files over to its replacement.
    Commit,
}
impl StepKind {
    fn writes_files(self) -> bool {
        self != Self::Config
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct Step {
    /// The instruction without the shell and build arguments the builder recorded around it.
    pub instruction: String,
    pub kind: StepKind,
    /// Bytes the step wrote, unpacked.
    pub size: u64,
    /// Unix seconds; `None` when Docker records no time.
    pub created: Option<u64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub comment: String,
    /// Whether the step wrote one of the image's layers.
    pub layer: bool,
    /// How many of the image's layers lie at or below this step.
    #[serde(skip)]
    pub chain: usize,
}

/// Docker's view of one image ID.
#[derive(Debug, PartialEq)]
pub(crate) struct Image {
    /// The size Docker reports. The containerd image store also counts the compressed copy of
    /// each layer that it keeps beside the unpacked one.
    pub size: u64,
    /// The root filesystem's layers, the lowest first.
    pub layers: Vec<String>,
    /// The history, the oldest step first.
    pub steps: Vec<Step>,
}
impl Image {
    pub(crate) fn new(size: u64, layers: Vec<String>, mut steps: Vec<Step>) -> Self {
        assign_layers(&mut steps, layers.len());
        Self {
            size,
            layers,
            steps,
        }
    }
    /// What the layers hold unpacked.
    pub(crate) fn layers_size(&self) -> u64 {
        self.steps.iter().map(|step| step.size).sum()
    }
    /// Whether a step that wrote no layer is here too, on the same layers.
    fn has_setting(&self, step: &Step) -> bool {
        self.steps.iter().any(|own| {
            !own.layer
                && own.chain == step.chain
                && own.instruction == step.instruction
                && own.comment == step.comment
        })
    }
}

/// Mark the steps that wrote a layer. History has no such flag: a step with bytes wrote one, and
/// so may a step without bytes that is not a setting, when only counting those as well gives as
/// many layers as the root filesystem has.
pub(crate) fn assign_layers(steps: &mut [Step], layers: usize) {
    let written = steps.iter().filter(|step| step.size > 0).count();
    let possible = steps
        .iter()
        .filter(|step| step.size > 0 || step.kind.writes_files())
        .count();
    let empty_layers = written != layers && possible == layers;
    let mut chain = 0;
    for step in steps {
        step.layer =
            chain < layers && (step.size > 0 || (empty_layers && step.kind.writes_files()));
        if step.layer {
            chain += 1;
        }
        step.chain = chain;
    }
}

/// `docker image inspect --format '{{.Size}} {{json .RootFS.Layers}}'`.
pub(crate) fn parse_inspect(text: &str) -> anyhow::Result<(u64, Vec<String>)> {
    let (size, layers) = text
        .trim()
        .split_once(' ')
        .context("Docker described the image in an unexpected form")?;
    let layers: Option<Vec<String>> = serde_json::from_str(layers)?;
    Ok((size.parse()?, layers.unwrap_or_default()))
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct HistoryLine {
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    created_by: String,
    #[serde(default)]
    comment: String,
    size: String,
}

/// `docker image history --no-trunc --human=false --format '{{json .}}'`, which lists the newest
/// step first, as steps with the oldest first.
pub(crate) fn parse_history(text: &str) -> anyhow::Result<Vec<Step>> {
    let mut steps = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(history_step)
        .collect::<anyhow::Result<Vec<_>>>()?;
    steps.reverse();
    Ok(steps)
}

fn history_step(line: &str) -> anyhow::Result<Step> {
    let entry: HistoryLine = serde_json::from_str(line)?;
    let (kind, instruction) = describe(&entry.created_by, &entry.comment);
    let size = entry
        .size
        .parse()
        .with_context(|| format!("Docker reported a step size of {}", entry.size))?;
    Ok(Step {
        instruction: shorten(instruction),
        kind,
        size,
        created: unix_time(&entry.created_at),
        comment: if entry.comment == BUILDKIT {
            String::new()
        } else {
            entry.comment
        },
        layer: false,
        chain: 0,
    })
}

fn shorten(mut text: String) -> String {
    if text.len() > INSTRUCTION_LIMIT {
        let mut end = INSTRUCTION_LIMIT;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push('…');
    }
    text
}

/// What a history entry did, and its instruction as a Dockerfile would say it.
pub(crate) fn describe(created_by: &str, comment: &str) -> (StepKind, String) {
    let text = created_by.trim();
    // BuildKit records the Dockerfile instruction itself.
    let buildkit = text
        .strip_suffix("# buildkit")
        .map(str::trim_end)
        .or((comment == BUILDKIT).then_some(text));
    if let Some(instruction) = buildkit {
        let (keyword, arguments) = instruction.split_once(' ').unwrap_or((instruction, ""));
        return match keyword_kind(keyword) {
            StepKind::Run => (StepKind::Run, format!("RUN {}", shell_command(arguments))),
            kind => (kind, instruction.to_owned()),
        };
    }
    // The classic builder marks every instruction but RUN as `#(nop)`.
    if let Some(instruction) = text.strip_prefix("/bin/sh -c #(nop)").map(str::trim) {
        let keyword = instruction.split_whitespace().next().unwrap_or_default();
        return (keyword_kind(keyword), instruction.to_owned());
    }
    if text.starts_with('|') || text.starts_with("/bin/sh -c ") {
        return (StepKind::Run, format!("RUN {}", shell_command(text)));
    }
    // `docker commit` records the container's command, or nothing.
    (StepKind::Commit, text.to_owned())
}

fn keyword_kind(keyword: &str) -> StepKind {
    match keyword.to_ascii_uppercase().as_str() {
        "RUN" => StepKind::Run,
        "COPY" => StepKind::Copy,
        "ADD" => StepKind::Add,
        _ => StepKind::Config,
    }
}

/// A RUN step's command without the shell and the build arguments the step saw, which builders
/// record as `|2 NAME=value NAME=value /bin/sh -c command`.
fn shell_command(arguments: &str) -> &str {
    let arguments = arguments.trim();
    match arguments.split_once("/bin/sh -c ") {
        Some((before, command)) if before.is_empty() || before.starts_with('|') => command.trim(),
        _ => arguments,
    }
}

/// Unix seconds of the RFC 3339 time `docker image history --human=false` prints; `None` for a
/// time before 1970, which Docker uses when an image records none.
pub(crate) fn unix_time(text: &str) -> Option<u64> {
    let number = |range| digits(text, range);
    let bytes = text.as_bytes();
    if bytes.len() < 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !matches!(bytes[10], b'T' | b't' | b' ')
        || bytes[13] != b':'
        || bytes[16] != b':'
    {
        return None;
    }
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let mut zone = &text[19..];
    if let Some(fraction) = zone.strip_prefix('.') {
        zone = fraction.trim_start_matches(|c: char| c.is_ascii_digit());
    }
    let offset = match zone {
        "Z" | "z" => 0,
        _ => {
            let sign = match zone.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            if zone.len() != 6 || zone.as_bytes()[3] != b':' {
                return None;
            }
            sign * (digits(zone, 1..3)? * 3600 + digits(zone, 4..6)? * 60)
        }
    };
    // Days since 1970-01-01 in the proleptic Gregorian calendar, years starting in March.
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    u64::try_from(days * 86_400 + hour * 3600 + minute * 60 + second - offset).ok()
}

/// The decimal number in `text[range]`, digits only.
fn digits(text: &str, range: std::ops::Range<usize>) -> Option<i64> {
    let part = text.get(range)?;
    part.bytes()
        .all(|byte| byte.is_ascii_digit())
        .then(|| part.parse().ok())
        .flatten()
}

/// An image the list shows, with Docker's view of it.
pub(crate) struct Listed {
    /// The list's ID: a catalog ID or a record ID.
    pub id: String,
    pub image_id: String,
    pub catalog: bool,
    pub created_at: u64,
    pub image: Arc<Image>,
}
impl Listed {
    /// The image a shared step comes from has fewer layers, or fewer steps on the same layers.
    /// Among identical images the catalog's comes first, then the oldest.
    fn rank(&self) -> (usize, usize, bool, u64, &str) {
        (
            self.image.layers.len(),
            self.image.steps.len(),
            !self.catalog,
            self.created_at,
            &self.id,
        )
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct Origin {
    /// The image the step comes from, `None` when it is this image's own.
    pub from: Option<usize>,
    /// Every other image that has the step.
    pub shared_with: Vec<usize>,
}

/// Where each step of `images[index]` comes from, the oldest step first.
pub(crate) fn origins(images: &[Listed], index: usize) -> Vec<Origin> {
    let own = &images[index];
    let common: Vec<usize> = images
        .iter()
        .map(|other| {
            own.image
                .layers
                .iter()
                .zip(&other.image.layers)
                .take_while(|(a, b)| a == b)
                .count()
        })
        .collect();
    own.image
        .steps
        .iter()
        .map(|step| {
            let shared_with: Vec<usize> = (0..images.len())
                .filter(|&other| {
                    other != index
                        && common[other] >= step.chain
                        && (step.layer || images[other].image.has_setting(step))
                })
                .collect();
            let from = shared_with
                .iter()
                .copied()
                .min_by(|&a, &b| images[a].rank().cmp(&images[b].rank()))
                .filter(|&other| images[other].rank() < own.rank());
            Origin { from, shared_with }
        })
        .collect()
}

/// The bytes of the steps that are the image's own.
pub(crate) fn own_size(image: &Image, origins: &[Origin]) -> u64 {
    image
        .steps
        .iter()
        .zip(origins)
        .filter(|(_, origin)| origin.from.is_none())
        .map(|(step, _)| step.size)
        .sum()
}

/// An image's steps, the newest first as `docker history` lists them, and its sizes.
pub(crate) fn report(images: &[Listed], index: usize) -> Value {
    let listed = &images[index];
    let origins = origins(images, index);
    let steps: Vec<Value> = listed
        .image
        .steps
        .iter()
        .zip(&origins)
        .rev()
        .map(|(step, origin)| {
            let mut value = json!(step);
            value["from"] = json!(origin.from.map(|other| &images[other].id));
            value["shared_with"] = json!(
                origin
                    .shared_with
                    .iter()
                    .map(|&other| &images[other].id)
                    .collect::<Vec<_>>()
            );
            value
        })
        .collect();
    json!({
        "id": listed.id,
        "image_id": listed.image_id,
        "size": listed.image.size,
        "layers_size": listed.image.layers_size(),
        "own_size": own_size(&listed.image, &origins),
        "steps": steps,
    })
}

impl App {
    /// Docker's view of these image IDs, read once per ID. Images Docker cannot read are left
    /// out, and images no longer asked for leave the cache.
    async fn read_images(&self, ids: &[&str]) -> HashMap<String, Arc<Image>> {
        let unread: HashSet<&str> = {
            let cache = self.layers.0.lock().unwrap();
            ids.iter()
                .copied()
                .filter(|id| !cache.contains_key(*id))
                .collect()
        };
        let read = futures_util::future::join_all(
            unread
                .into_iter()
                .map(|id| async move { (id, self.read_image(id).await) }),
        )
        .await;
        let mut cache = self.layers.0.lock().unwrap();
        for (id, result) in read {
            match result {
                Ok(image) => {
                    cache.insert(id.to_owned(), Arc::new(image));
                }
                Err(error) => tracing::debug!("Reading the layers of {id}: {error:#}"),
            }
        }
        let wanted: HashSet<&str> = ids.iter().copied().collect();
        cache.retain(|id, _| wanted.contains(id.as_str()));
        cache.clone()
    }
    async fn read_image(&self, id: &str) -> anyhow::Result<Image> {
        let inspect = [
            "image",
            "inspect",
            "--format",
            "{{.Size}} {{json .RootFS.Layers}}",
            id,
        ];
        let history = [
            "image",
            "history",
            "--no-trunc",
            "--human=false",
            "--format",
            "{{json .}}",
            id,
        ];
        let (inspected, history) = tokio::join!(self.docker(&inspect), self.docker(&history));
        let (size, layers) = parse_inspect(&String::from_utf8_lossy(&inspected?))?;
        let steps = parse_history(&String::from_utf8_lossy(&history?))?;
        Ok(Image::new(size, layers, steps))
    }
    /// The images in an image list that Docker has, with its view of each.
    async fn listed(&self, entries: &[Value]) -> Vec<Listed> {
        let present: Vec<(&Value, &str)> = entries
            .iter()
            .filter(|entry| entry["available"] == true || entry["downloaded"] == true)
            .filter_map(|entry| Some((entry, entry["image_id"].as_str()?)))
            .collect();
        let ids: Vec<&str> = present.iter().map(|(_, id)| *id).collect();
        let images = self.read_images(&ids).await;
        present
            .into_iter()
            .filter_map(|(entry, image_id)| {
                Some(Listed {
                    id: entry["id"].as_str()?.to_owned(),
                    image_id: image_id.to_owned(),
                    catalog: entry["kind"] == "catalog",
                    created_at: entry["created_at"].as_u64().unwrap_or_default(),
                    image: images.get(image_id)?.clone(),
                })
            })
            .collect()
    }
}

/// Give each image of a list its sizes: `size` as Docker reports it, `layers_size` unpacked and
/// `own_size`, the part of that no other listed image brought. All three are `null` for an image
/// Docker does not have.
pub(crate) async fn add_sizes(app: &App, entries: &mut [Value]) {
    let listed = app.listed(entries).await;
    let sizes: HashMap<&str, [u64; 3]> = (0..listed.len())
        .map(|index| {
            let image = &listed[index].image;
            let own = own_size(image, &origins(&listed, index));
            (
                listed[index].id.as_str(),
                [image.size, image.layers_size(), own],
            )
        })
        .collect();
    for entry in entries.iter_mut() {
        let found = entry["id"].as_str().and_then(|id| sizes.get(id)).copied();
        for (position, key) in ["size", "layers_size", "own_size"].into_iter().enumerate() {
            entry[key] = json!(found.map(|sizes| sizes[position]));
        }
    }
}

/// `GET /api/images/{id}/layers`: what an image's space is made of.
pub async fn layers(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    let entries = crate::api::image_list(&app).await?;
    if !entries.iter().any(|entry| entry["id"] == id.as_str()) {
        return Err(missing());
    }
    let listed = app.listed(&entries).await;
    let index = listed
        .iter()
        .position(|image| image.id == id)
        .ok_or_else(|| {
            Error(
                StatusCode::NOT_FOUND,
                "Docker does not have this image".into(),
            )
        })?;
    Ok(Json(report(&listed, index)))
}

#[cfg(test)]
#[path = "layers_tests.rs"]
mod tests;
