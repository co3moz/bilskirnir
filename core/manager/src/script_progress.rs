//! Where the scripts of a build or a compose setup are, from what they say about themselves and
//! from what the manager learns on its own.
//!
//! A script may print `THORIC_STAGE=<i>/<n> <label>` when it begins the i-th of its n steps, and
//! `THORIC_SKIP=<i>/<n> <label>` for a step it leaves out, such as an option that is off. Without
//! those, the line trace bash reads first reports each line further down the script reaches as
//! `THORIC_LINE=<line>`: that line of the script names what runs and its position is the estimate.
//! The trace's lines stay out of the log, the script's own markers stay in it.
//!
//! A build's parts and a compose's steps are a script's parts, each weighing the same. BuildKit
//! names the part a build step runs in the step's first line and ends it with `DONE`, or `CACHED`
//! when an earlier build's layer is reused. The job's `progress` is the running part's latest
//! step with a percentage that never goes back, and its `script` report keeps every part's times
//! and named steps once the job ends. A number a script jumped over, and the numbers after its
//! last step in a part that finished, were skipped; they are left out of the report.
use crate::{App, model::Job};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;

/// The line trace bash reads through `BASH_ENV` before a build or compose setup script.
pub(crate) const TRACE: &str = include_str!("../helpers/script_trace.sh");
/// The trace's name in a build's context and under `/tmp` while a part runs: a part's id has no
/// underscore, so no part's script can take it.
pub(crate) const TRACE_NAME: &str = "thoric_trace.sh";
/// The steps a part's report keeps; the progress goes on past them.
const STEP_LIMIT: usize = 200;
const LABEL_LIMIT: usize = 120;
const STAGE_LIMIT: u32 = 10_000;

/// What the job keeps of its scripts' parts and the steps they named.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ScriptReport {
    pub parts: Vec<PartReport>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PartReport {
    /// The part's name, when a build has several parts or a compose several steps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    /// When the part ended well. One that a failed or interrupted job stopped in has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<u64>,
    /// BuildKit reused an earlier build's layer, so the script did not run.
    #[serde(default, skip_serializing_if = "is_false")]
    pub cached: bool,
    /// How many steps the script's markers counted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stages: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<ScriptStep>,
}

/// A step a script named. A skipped one has no times.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScriptStep {
    pub stage: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<u64>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub skipped: bool,
}

fn is_false(value: &bool) -> bool {
    !value
}

/// A line of a script's output that reports where it is.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Marker {
    Step(Step),
    /// The trace reached this line of the script.
    Line(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Step {
    stage: u32,
    stages: u32,
    label: Option<String>,
    skipped: bool,
}

/// The marker a line of a script's output is, if any. A marker is the whole line.
pub(crate) fn marker(text: &str) -> Option<Marker> {
    if let Some(line) = text.strip_prefix("THORIC_LINE=") {
        return number(line)
            .filter(|line| *line > 0)
            .map(|line| Marker::Line(line as usize));
    }
    let (rest, skipped) = match text.strip_prefix("THORIC_STAGE=") {
        Some(rest) => (rest, false),
        None => (text.strip_prefix("THORIC_SKIP=")?, true),
    };
    let (count, label) = rest.split_once(' ').unwrap_or((rest, ""));
    let (stage, stages) = count.split_once('/')?;
    let (stage, stages) = (number(stage)?, number(stages)?);
    if stage == 0 || stage > stages || stages > STAGE_LIMIT {
        return None;
    }
    Some(Marker::Step(Step {
        stage,
        stages,
        label: clean(label),
        skipped,
    }))
}

fn number(text: &str) -> Option<u32> {
    if text.is_empty() || text.len() > 9 || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// A label of one line without control characters, shortened to the limit; none when empty.
fn clean(text: &str) -> Option<String> {
    let text = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>();
    let text = text.trim();
    (!text.is_empty()).then(|| {
        text.chars()
            .take(LABEL_LIMIT)
            .collect::<String>()
            .trim_end()
            .to_owned()
    })
}

/// A line of BuildKit's plain progress output: its step number and the rest.
fn build_step(line: &str) -> Option<(&str, &str)> {
    let (step, rest) = line.strip_prefix('#')?.split_once(' ')?;
    (!step.is_empty() && step.bytes().all(|byte| byte.is_ascii_digit())).then_some((step, rest))
}

/// The output of the command a build step runs: BuildKit writes the seconds since the step
/// started before it.
fn step_output(rest: &str) -> Option<&str> {
    let (time, text) = rest.split_once(' ').unwrap_or((rest, ""));
    let (whole, fraction) = time.split_once('.')?;
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    (digits(whole) && digits(fraction)).then_some(text)
}

/// The id of the part a build step runs, from the step's first line.
fn step_part(rest: &str) -> Option<&str> {
    if !rest.starts_with('[') || !rest.contains("] RUN ") {
        return None;
    }
    let (_, script) = rest.split_once(" bash /tmp/thoric-")?;
    script.split_once(".sh").map(|(id, _)| id)
}

/// A script and the part of a build or a compose it is.
pub(crate) struct ScriptPart {
    pub id: String,
    pub name: String,
    pub script: String,
}

#[derive(Clone, Copy, PartialEq)]
enum Form {
    /// BuildKit's plain output of a build, a step number before each line.
    Build,
    /// A compose setup's output as the script wrote it, one step after the other.
    Setup,
}

#[derive(Default)]
struct PartState {
    /// The last step the script named.
    step: Option<Step>,
    /// The furthest line the trace reported.
    line: usize,
}

/// Follows the output of a build's or a compose setup's scripts.
pub(crate) struct ScriptProgress {
    form: Form,
    ids: Vec<String>,
    lines: Vec<Vec<String>>,
    states: Vec<PartState>,
    report: ScriptReport,
    /// The part each of BuildKit's steps runs.
    build_steps: HashMap<String, usize>,
    /// The part that started last.
    current: Option<usize>,
    percent: u32,
    ended: bool,
    /// The report changed since it was saved.
    changed: bool,
    /// The progress last saved.
    saved: Option<Value>,
}

impl ScriptProgress {
    pub(crate) fn build(parts: Vec<ScriptPart>) -> Self {
        Self::new(Form::Build, parts)
    }

    pub(crate) fn setup(parts: Vec<ScriptPart>) -> Self {
        Self::new(Form::Setup, parts)
    }

    fn new(form: Form, parts: Vec<ScriptPart>) -> Self {
        let named = parts.len() > 1;
        Self {
            form,
            report: ScriptReport {
                parts: parts
                    .iter()
                    .map(|part| PartReport {
                        name: named.then(|| part.name.clone()),
                        ..PartReport::default()
                    })
                    .collect(),
            },
            states: parts.iter().map(|_| PartState::default()).collect(),
            lines: parts
                .iter()
                .map(|part| part.script.lines().map(str::to_owned).collect())
                .collect(),
            ids: parts.into_iter().map(|part| part.id).collect(),
            build_steps: HashMap::new(),
            current: None,
            percent: 0,
            ended: false,
            changed: true,
            saved: None,
        }
    }

    /// Whether this follows a build, whose output also starts the job's later stages.
    pub(crate) fn builds(&self) -> bool {
        self.form == Form::Build
    }

    pub(crate) fn start_part(&mut self, index: usize, at: u64) {
        self.current = Some(index);
        let part = &mut self.report.parts[index];
        if part.started_at.is_none() {
            part.started_at = Some(at);
            self.changed = true;
        }
    }

    /// The part ended well.
    pub(crate) fn finish_part(&mut self, index: usize, at: u64) {
        let part = &mut self.report.parts[index];
        if part.finished_at.is_some() {
            return;
        }
        part.started_at.get_or_insert(at);
        part.finished_at = Some(at);
        close(part, at);
        self.changed = true;
    }

    /// The scripts' stage is over, so the job's progress is no longer theirs.
    pub(crate) fn end(&mut self) {
        self.ended = true;
    }

    /// Read one line of output. Returns whether it belongs in the log, which a trace line does not.
    pub(crate) fn read(&mut self, line: &str, at: u64) -> bool {
        let line = line.trim_end_matches(['\n', '\r']);
        if self.form == Form::Setup {
            return match self.current {
                Some(part) => self.output(part, line, at),
                None => true,
            };
        }
        let Some((step, rest)) = build_step(line) else {
            return true;
        };
        if let Some(index) =
            step_part(rest).and_then(|id| self.ids.iter().position(|part| part == id))
        {
            self.build_steps.insert(step.to_owned(), index);
            self.start_part(index, at);
            return true;
        }
        let Some(&part) = self.build_steps.get(step) else {
            return true;
        };
        if rest == "CACHED" {
            self.start_part(part, at);
            self.report.parts[part].cached = true;
            self.finish_part(part, at);
        } else if rest.starts_with("DONE ") {
            self.finish_part(part, at);
        } else if let Some(text) = step_output(rest) {
            return self.output(part, text, at);
        }
        true
    }

    fn output(&mut self, index: usize, text: &str, at: u64) -> bool {
        match marker(text) {
            Some(Marker::Line(line)) => {
                let state = &mut self.states[index];
                state.line = state.line.max(line);
                false
            }
            Some(Marker::Step(step)) => {
                self.step(index, step, at);
                true
            }
            None => true,
        }
    }

    /// A step the script named. Steps only move forward, so a repeated or earlier one changes
    /// nothing.
    fn step(&mut self, index: usize, step: Step, at: u64) {
        let state = &mut self.states[index];
        if state
            .step
            .as_ref()
            .is_some_and(|last| step.stage <= last.stage)
        {
            return;
        }
        let part = &mut self.report.parts[index];
        part.stages = Some(step.stages);
        close(part, at);
        if part.steps.len() < STEP_LIMIT {
            part.steps.push(ScriptStep {
                stage: step.stage,
                label: step.label.clone(),
                started_at: (!step.skipped).then_some(at),
                finished_at: None,
                skipped: step.skipped,
            });
        }
        state.step = Some(step);
        self.changed = true;
    }

    /// How much of part `index` is done, from 0 to 1.
    fn done(&self, index: usize) -> f64 {
        let part = &self.report.parts[index];
        if part.finished_at.is_some() {
            return 1.0;
        }
        if part.started_at.is_none() {
            return 0.0;
        }
        let state = &self.states[index];
        match &state.step {
            Some(step) => f64::from(step.stage - u32::from(!step.skipped)) / f64::from(step.stages),
            None if state.line == 0 || self.lines[index].is_empty() => 0.0,
            None => ((state.line - 1) as f64 / self.lines[index].len() as f64).min(1.0),
        }
    }

    /// The text of the command on `line` of part `index`'s script, a continued command named by
    /// its first line.
    fn command(&self, index: usize, line: usize) -> Option<String> {
        let lines = &self.lines[index];
        let mut at = line.checked_sub(1).filter(|at| *at < lines.len())?;
        while at > 0 && lines[at - 1].trim_end().ends_with('\\') {
            at -= 1;
        }
        clean(lines[at].trim_end().trim_end_matches('\\'))
    }

    /// The job's progress: the part that started last, its latest step or line, and the share
    /// of all parts done.
    pub(crate) fn summary(&mut self) -> Option<Value> {
        if self.ended {
            return None;
        }
        let index = self.current?;
        let parts = self.report.parts.len();
        let done = (0..parts).map(|part| self.done(part)).sum::<f64>();
        self.percent = self
            .percent
            .max(((done * 100.0) / parts as f64).floor().min(100.0) as u32);
        let part = &self.report.parts[index];
        let state = &self.states[index];
        let mut summary = json!({"part": index + 1, "parts": parts, "percent": self.percent});
        if let Some(name) = &part.name {
            summary["name"] = name.as_str().into();
        }
        if let Some(step) = &state.step {
            summary["stage"] = step.stage.into();
            summary["stages"] = step.stages.into();
            if let Some(label) = &step.label {
                summary["label"] = label.as_str().into();
            }
            if step.skipped {
                summary["skipped"] = true.into();
            }
        } else if state.line > 0 && part.finished_at.is_none() {
            summary["line"] = state.line.into();
            summary["lines"] = self.lines[index].len().into();
            if let Some(label) = self.command(index, state.line) {
                summary["label"] = label.into();
            }
        }
        Some(json!({ "script": summary }))
    }
}

/// End the step that is running in `part`.
fn close(part: &mut PartReport, at: u64) {
    if let Some(step) = part.steps.last_mut()
        && step.started_at.is_some()
        && step.finished_at.is_none()
    {
        step.finished_at = Some(at);
    }
}

impl App {
    /// Save what changed of the scripts' progress since the last save: the job's progress, which
    /// pages hear of, and its report.
    pub(crate) fn save_script(&self, id: &str, progress: &mut ScriptProgress) -> Result<()> {
        let summary = progress.summary();
        let moved = summary != progress.saved;
        if !moved && !progress.changed {
            return Ok(());
        }
        let mut job: Job = self.db.get("job", id)?;
        if progress.changed {
            job.script = Some(progress.report.clone());
        }
        if moved {
            job.progress = summary.clone();
        }
        self.db.put("job", id, &job)?;
        progress.changed = false;
        if moved {
            progress.saved = summary;
            self.events.progress(id, job.progress.as_ref());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "script_progress_tests.rs"]
mod tests;
