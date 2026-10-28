//! What an image built on a saved image keeps of that image's compose. The base's layers already
//! hold everything its own build installed, with the choices made then, so nothing of that build
//! runs or is asked for again. What a new container asks for and sets up is the base's compose,
//! so the built image keeps it: the base's steps, then its own, as **Merge and install** merges
//! templates. The one step of a compose that names none takes the id of the template the image
//! was built from, or `base` and `build`.
//!
//! A step of the image's own with the id of one of the base's, or with the same script and
//! parameters, takes its place, such as a template installed again on an image that has it. So
//! adding the base's compose to an image that has it changes nothing. An id chosen here for the
//! image's own step replaces nothing and takes the next free one instead.
use super::{Compose, Step, compose_image};
use crate::{
    App,
    api::{BuildImage, image_summary},
    error::{Error, Result},
    model::{BUILD_PART_LIMIT, ImageRecord},
};
use anyhow::anyhow;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::Value;
use std::sync::Arc;

/// The id of the base's step of a compose of one that names none, without a template.
const BASE: &str = "base";
/// The id of the image's own step of a compose of one that names none, without a template.
const OWN: &str = "build";

/// The id step `index` of `compose` goes by when composes are joined: its own, or for the one
/// step of a compose that names none, the template the image was built from, else `fallback`.
fn step_id<'a>(
    compose: &'a Compose,
    index: usize,
    templates: &'a [String],
    fallback: &'a str,
) -> &'a str {
    match (
        compose.steps.len(),
        compose.steps[index].id.as_str(),
        templates,
    ) {
        (1, "", [template]) => template,
        (_, "", _) => fallback,
        (_, id, _) => id,
    }
}

/// Whether two steps run the same thing, as a step copied by hand from the base does.
fn same_work(a: &Step, b: &Step) -> bool {
    a.script == b.script && a.parameters == b.parameters && a.root == b.root
}

/// For every step of the base's compose, the step of the image's own that takes its place.
fn matches(
    base: &Compose,
    base_templates: &[String],
    own: &Compose,
    own_templates: &[String],
) -> Vec<Option<usize>> {
    let mut taken = vec![false; own.steps.len()];
    (0..base.steps.len())
        .map(|index| {
            let id = step_id(base, index, base_templates, BASE);
            let found = (0..own.steps.len()).find(|&other| {
                !taken[other]
                    && (step_id(own, other, own_templates, "") == id
                        || same_work(&base.steps[index], &own.steps[other]))
            })?;
            taken[found] = true;
            Some(found)
        })
        .collect()
}

/// Step `index` of `compose` with the id it goes by, and for the one step of a compose that
/// names none, the image's name.
fn named(
    compose: &Compose,
    index: usize,
    templates: &[String],
    fallback: &str,
    image: &str,
) -> Step {
    let mut step = compose.steps[index].clone();
    step.id = step_id(compose, index, templates, fallback).into();
    if compose.steps.len() == 1 && step.name.is_empty() {
        step.name = image.replace(['\n', '\r'], " ");
    }
    step
}

/// The compose of an image whose own is `own`, built from `templates` and named `name`, on the
/// image `base`: the base's steps, then its own. `own` as it is when it already has every step
/// of the base's.
pub(crate) fn extend(
    base: Option<&ImageRecord>,
    own: Option<Compose>,
    templates: &[String],
    name: &str,
) -> anyhow::Result<Option<Compose>> {
    let Some((record, inherited)) =
        base.and_then(|record| Some((record, record.compose.as_ref()?)))
    else {
        return Ok(own);
    };
    let Some(own) = own else {
        return Ok(Some(inherited.clone()));
    };
    let matched = matches(inherited, &record.template_ids, &own, templates);
    if matched.iter().all(Option::is_some) {
        return Ok(Some(own));
    }
    let mut steps = matched
        .iter()
        .enumerate()
        .map(|(index, found)| {
            let base_id = step_id(inherited, index, &record.template_ids, BASE);
            match found {
                Some(other) => named(&own, *other, templates, base_id, name),
                None => named(inherited, index, &record.template_ids, BASE, &record.name),
            }
        })
        .collect::<Vec<_>>();
    steps.extend(
        (0..own.steps.len())
            .filter(|other| !matched.contains(&Some(*other)))
            .map(|other| named(&own, other, templates, "", name)),
    );
    while let Some(index) = steps.iter().position(|step| step.id.is_empty()) {
        steps[index].id = free(OWN, &steps);
    }
    let compose = Compose { steps };
    compose.validate()?;
    Ok(Some(compose))
}

/// `id`, or the first of `id-2`, `id-3` and on that no step has.
fn free(id: &str, steps: &[Step]) -> String {
    let taken = |candidate: &str| steps.iter().any(|step| step.id == candidate);
    std::iter::once(id.to_owned())
        .chain((2..).map(|n| format!("{id}-{n}")))
        .find(|candidate| !taken(candidate))
        .unwrap_or_default()
}

/// The templates of an image built on one from `base`: the base's, then its own, each once,
/// the newest when there are more than a build may name.
fn joined(base: &[String], own: &[String]) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for id in base.iter().chain(own) {
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    ids.split_off(ids.len().saturating_sub(BUILD_PART_LIMIT))
}

/// The record of the image a build starts from, `requested` as the request named it and
/// `image_id` as Docker resolved it: the one whose compose a new container would get, otherwise
/// any record of that image.
fn base_record<'a>(
    records: &'a [ImageRecord],
    requested: &str,
    image_id: &str,
) -> Option<&'a ImageRecord> {
    compose_image(records, requested, image_id).or_else(|| {
        records.iter().find(|record| {
            record.image_id == image_id
                || record.image_id == requested
                || record.reference == requested
        })
    })
}

/// The compose and templates of the image `body` builds on `parent`, the base's Docker ID,
/// which the job's log names when the base brings a compose.
pub(crate) fn built_on(
    app: &App,
    job: &str,
    body: &BuildImage,
    parent: &str,
) -> anyhow::Result<(Option<Compose>, Vec<String>)> {
    let templates = body.templates();
    if !body.inherit_compose {
        return Ok((body.compose.clone(), templates));
    }
    let records: Vec<ImageRecord> = app.db.list("image")?;
    let Some(base) = base_record(&records, &body.base_image, parent) else {
        return Ok((body.compose.clone(), templates));
    };
    let compose = extend(Some(base), body.compose.clone(), &templates, &body.name).map_err(|error| {
        anyhow!(
            "The compose of {} and this build's do not fit together: {error:#}. Build without the base image's compose (inherit_compose false) to leave it out",
            base.name
        )
    })?;
    if let Some(inherited) = &base.compose {
        app.append_log(
            job,
            &format!(
                "Keeping the compose of {}, which new containers from it ask for and run: {}.",
                base.name,
                inherited
                    .steps
                    .iter()
                    .map(|step| match step.title() {
                        "" => base.name.as_str(),
                        title => title,
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )?;
    }
    Ok((compose, joined(&base.template_ids, &templates)))
}

/// The image `record` was built on, when it has a compose: builds made before they kept their
/// base's compose dropped it.
fn base_of<'a>(records: &'a [ImageRecord], record: &ImageRecord) -> Option<&'a ImageRecord> {
    let parent = record
        .parent_image_id
        .as_deref()
        .filter(|_| record.kind == "build")?;
    compose_image(records, parent, parent).filter(|base| base.id != record.id)
}

/// Whether `record` lacks a step of the compose of the image it was built on, which
/// `POST /api/images/{id}/compose/base` adds. The image list shows it as `base_compose_missing`.
pub(crate) fn base_compose_missing(records: &[ImageRecord], record: &ImageRecord) -> bool {
    let Some(base) = base_of(records, record) else {
        return false;
    };
    let Some(inherited) = &base.compose else {
        return false;
    };
    match &record.compose {
        None => true,
        Some(own) => matches(inherited, &base.template_ids, own, &record.template_ids)
            .iter()
            .any(Option::is_none),
    }
}

fn conflict(message: &str) -> Error {
    Error(StatusCode::CONFLICT, message.into())
}

/// `POST /api/images/{id}/compose/base`: give an image built on a saved image the steps of that
/// image's compose it lacks, before its own, without building it again. Containers keep the
/// compose they were created with.
pub async fn add_base(State(app): State<Arc<App>>, Path(id): Path<String>) -> Result<Json<Value>> {
    let _guard = app.hold(&format!("image:{id}")).await?;
    let records: Vec<ImageRecord> = app.db.list("image")?;
    let mut record: ImageRecord = app.db.get("image", &id)?;
    let base = base_of(&records, &record)
        .ok_or_else(|| conflict("The image was not built on a saved image with a compose"))?;
    let compose = extend(
        Some(base),
        record.compose.clone(),
        &record.template_ids,
        &record.name,
    )?;
    if compose == record.compose {
        return Err(conflict(
            "The image already has every step of the compose of the image it was built on",
        ));
    }
    record.template_ids = joined(&base.template_ids, &record.template_ids);
    record.compose = compose;
    app.db.put("image", &id, &record)?;
    Ok(Json(image_summary(&record)))
}

#[cfg(test)]
#[path = "inherit_tests.rs"]
mod tests;
