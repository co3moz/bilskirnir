//! Pins down how the scripts of a build or a compose setup report where they are: a marker is a
//! whole line within bounds, steps only move forward, a skipped step counts as done, a script's
//! own markers outrank the line trace, the trace never reaches the log, BuildKit's steps are
//! matched to the parts they run, and the percentage never goes back.
use super::*;

fn part(id: &str, name: &str, script: &str) -> ScriptPart {
    ScriptPart {
        id: id.into(),
        name: name.into(),
        script: script.into(),
    }
}

fn step(stage: u32, stages: u32, label: Option<&str>, skipped: bool) -> Marker {
    Marker::Step(Step {
        stage,
        stages,
        label: label.map(str::to_owned),
        skipped,
    })
}

/// Reads every line of `output` at time `at`, returning the lines kept for the log.
fn read(progress: &mut ScriptProgress, output: &str, at: u64) -> Vec<String> {
    output
        .lines()
        .filter(|line| progress.read(line, at))
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_marker_is_a_whole_line_within_bounds() {
    assert_eq!(
        marker("THORIC_STAGE=3/10 Installing Ghidra"),
        Some(step(3, 10, Some("Installing Ghidra"), false))
    );
    assert_eq!(
        marker("THORIC_SKIP=7/10 Wine"),
        Some(step(7, 10, Some("Wine"), true))
    );
    assert_eq!(
        marker("THORIC_STAGE=10/10"),
        Some(step(10, 10, None, false))
    );
    assert_eq!(marker("THORIC_LINE=42"), Some(Marker::Line(42)));
    // Control characters become spaces and a long label is cut.
    assert_eq!(
        marker("THORIC_STAGE=1/2  \u{1b}[1mBold\u{1b}[0m\t "),
        Some(step(1, 2, Some("[1mBold [0m"), false))
    );
    let long = format!("THORIC_STAGE=1/1 {}", "x".repeat(500));
    let Some(Marker::Step(Step {
        label: Some(label), ..
    })) = marker(&long)
    else {
        panic!("{long}");
    };
    assert_eq!(label.chars().count(), LABEL_LIMIT);
    for line in [
        "THORIC_STAGE=0/3",
        "THORIC_STAGE=4/3",
        "THORIC_STAGE=3",
        "THORIC_STAGE=/3",
        "THORIC_STAGE=1/10001",
        "THORIC_STAGE=-1/3",
        "THORIC_STAGE=1.5/3",
        "THORIC_STAGE= 1/3",
        " THORIC_STAGE=1/3",
        "echo THORIC_STAGE=1/3",
        "THORIC_STAGES=1/3",
        "THORIC_LINE=0",
        "THORIC_LINE=12a",
        "THORIC_LINE=",
        "Setting up python3",
    ] {
        assert_eq!(marker(line), None, "{line}");
    }
}

/// `docker build --progress=plain` of three parts: the first reused from an earlier build, the
/// second reporting its steps, the third only traced. Newer BuildKit names the stage in a step's
/// first line, as `[stage-0 5/7]`.
const PARTS: &str = "#5 [2/7] COPY base.sh /tmp/thoric-base.sh
#5 CACHED

#6 [3/7] RUN --mount=type=bind,source=thoric_trace.sh,target=/tmp/thoric_trace.sh BASH_ENV=/tmp/thoric_trace.sh bash /tmp/thoric-base.sh && rm /tmp/thoric-base.sh
#6 CACHED

#7 [4/7] COPY ghidra.sh /tmp/thoric-ghidra.sh
#7 DONE 0.0s

#8 [stage-0 5/7] RUN --mount=type=bind,source=thoric_trace.sh,target=/tmp/thoric_trace.sh BASH_ENV=/tmp/thoric_trace.sh bash /tmp/thoric-ghidra.sh && rm /tmp/thoric-ghidra.sh
#8 0.101 THORIC_LINE=1
#8 0.102 THORIC_STAGE=1/4 Downloading Ghidra
#8 0.150 THORIC_LINE=3
#8 3.000 Ghidra downloaded
#8 3.001 THORIC_SKIP=2/4 Wine
#8 3.002 THORIC_STAGE=4/4 Linking
#8 DONE 5.0s

#9 [6/7] COPY gdb.sh /tmp/thoric-gdb.sh
#9 DONE 0.0s

#10 [stage-0 7/7] RUN --mount=type=bind,source=thoric_trace.sh,target=/tmp/thoric_trace.sh BASH_ENV=/tmp/thoric_trace.sh bash /tmp/thoric-gdb.sh && rm /tmp/thoric-gdb.sh
#10 0.100 THORIC_LINE=2
";

fn parts() -> ScriptProgress {
    ScriptProgress::build(vec![
        part(
            "base",
            "Base tools",
            "apt-get update\napt-get install -y curl\n",
        ),
        part("ghidra", "Ghidra", "a\nb\nc\nd\ne\nf\n"),
        part(
            "gdb",
            "GDB",
            "set -eu\napt-get install -y \\\n  gdb\necho done\n",
        ),
    ])
}

#[test]
fn a_build_follows_each_part_from_the_step_that_runs_it() {
    let mut progress = parts();
    let (ghidra, gdb) = (
        PARTS.find("#7 [4/7]").unwrap(),
        PARTS.find("#9 [6/7]").unwrap(),
    );
    // The manager's clock times the parts, not BuildKit's seconds.
    let mut kept = read(&mut progress, &PARTS[..ghidra], 10);
    kept.extend(read(&mut progress, &PARTS[ghidra..gdb], 20));
    kept.extend(read(&mut progress, &PARTS[gdb..], 30));
    // The trace's lines stay out of the log, everything else stays in it.
    assert!(kept.iter().all(|line| !line.contains("THORIC_LINE")));
    assert_eq!(kept.len(), PARTS.lines().count() - 3, "{kept:#?}");
    let report = &progress.report;
    assert_eq!(
        report.parts[0],
        PartReport {
            name: Some("Base tools".into()),
            started_at: Some(10),
            finished_at: Some(10),
            cached: true,
            ..PartReport::default()
        }
    );
    let ghidra = &report.parts[1];
    assert_eq!(
        (ghidra.started_at, ghidra.finished_at, ghidra.stages),
        (Some(20), Some(20), Some(4))
    );
    assert_eq!(
        ghidra.steps,
        [
            ScriptStep {
                stage: 1,
                label: Some("Downloading Ghidra".into()),
                started_at: Some(20),
                finished_at: Some(20),
                skipped: false,
            },
            ScriptStep {
                stage: 2,
                label: Some("Wine".into()),
                started_at: None,
                finished_at: None,
                skipped: true,
            },
            ScriptStep {
                stage: 4,
                label: Some("Linking".into()),
                started_at: Some(20),
                finished_at: Some(20),
                skipped: false,
            },
        ]
    );
    assert_eq!(
        (report.parts[2].started_at, report.parts[2].finished_at),
        (Some(30), None)
    );
    // The running part is traced: its continued command is named by its first line, and the
    // two parts before it count whole.
    assert_eq!(
        progress.summary(),
        Some(serde_json::json!({"script": {
            "part": 3, "parts": 3, "name": "GDB", "percent": 75,
            "line": 2, "lines": 4, "label": "apt-get install -y",
        }}))
    );
    // Its end ends the build's share of the work.
    read(&mut progress, "#10 DONE 1.0s", 40);
    assert_eq!(progress.summary().unwrap()["script"]["percent"], 100);
    progress.end();
    assert_eq!(progress.summary(), None);
    // The report is what the job keeps, and a step that never ran carries no times.
    let saved = serde_json::to_value(&progress.report).unwrap();
    assert_eq!(
        saved["parts"][1]["steps"][1],
        serde_json::json!({"stage": 2, "label": "Wine", "skipped": true})
    );
    assert_eq!(saved["parts"][0]["cached"], true);
    assert!(saved["parts"][1].get("cached").is_none());
}

#[test]
fn a_scripts_own_steps_outrank_the_trace_and_the_percentage_never_goes_back() {
    let script = "a\n".repeat(10);
    let mut progress = ScriptProgress::setup(vec![part("setup", "Setup", &script)]);
    assert_eq!(progress.summary(), None, "nothing runs yet");
    progress.start_part(0, 1);
    assert_eq!(read(&mut progress, "THORIC_LINE=9\nhello", 2), ["hello"]);
    let summary = progress.summary().unwrap();
    assert_eq!(summary["script"]["percent"], 80);
    assert_eq!(summary["script"]["label"], "a");
    // A single part has no name of its own.
    assert!(summary["script"].get("name").is_none());
    read(
        &mut progress,
        "THORIC_STAGE=1/5 Preparing\nTHORIC_LINE=10",
        3,
    );
    let summary = progress.summary().unwrap()["script"].clone();
    assert_eq!(
        summary,
        serde_json::json!({"part": 1, "parts": 1, "percent": 80, "stage": 1, "stages": 5, "label": "Preparing"})
    );
    // A repeated or earlier step changes nothing; a skipped one counts as done.
    read(
        &mut progress,
        "THORIC_STAGE=1/5 Again\nTHORIC_SKIP=4/5 Extras",
        4,
    );
    let summary = progress.summary().unwrap()["script"].clone();
    assert_eq!(summary["stage"], 4);
    assert_eq!(summary["skipped"], true);
    assert_eq!(summary["label"], "Extras");
    assert_eq!(summary["percent"], 80);
    read(&mut progress, "THORIC_STAGE=5/5 Last", 5);
    assert_eq!(progress.summary().unwrap()["script"]["percent"], 80);
    let steps = &progress.report.parts[0].steps;
    assert_eq!(
        steps.iter().map(|step| step.stage).collect::<Vec<_>>(),
        [1, 4, 5]
    );
    assert_eq!(
        (steps[0].started_at, steps[0].finished_at),
        (Some(3), Some(4))
    );
}

#[test]
fn a_compose_setup_follows_its_steps_and_one_that_fails_keeps_no_end() {
    let mut progress = ScriptProgress::setup(vec![
        part("chrome", "Google Chrome", "echo chrome\n"),
        part("vscode", "Visual Studio Code", "echo one\necho two\n"),
    ]);
    // Before a step runs, its output is not the script's.
    assert!(progress.read("THORIC_LINE=1", 1));
    progress.start_part(0, 1);
    assert!(!progress.read("THORIC_LINE=1\r\n", 2));
    assert!(progress.read("chrome\r\n", 2));
    progress.finish_part(0, 3);
    progress.start_part(1, 4);
    read(
        &mut progress,
        "THORIC_STAGE=1/2 Extensions\nTHORIC_LINE=2",
        5,
    );
    let summary = progress.summary().unwrap()["script"].clone();
    assert_eq!(
        summary,
        serde_json::json!({"part": 2, "parts": 2, "name": "Visual Studio Code", "percent": 50, "stage": 1, "stages": 2, "label": "Extensions"})
    );
    let report = &progress.report;
    assert_eq!(
        (report.parts[0].started_at, report.parts[0].finished_at),
        (Some(1), Some(3))
    );
    assert_eq!(
        (report.parts[1].started_at, report.parts[1].finished_at),
        (Some(4), None)
    );
    assert_eq!(report.parts[1].steps[0].finished_at, None);
}

#[test]
fn a_part_keeps_a_bounded_list_of_steps_while_its_progress_goes_on() {
    let mut progress = ScriptProgress::setup(vec![part("many", "Many", "true\n")]);
    progress.start_part(0, 1);
    let output = (1..=1000)
        .map(|stage| format!("THORIC_STAGE={stage}/1000 Step {stage}"))
        .collect::<Vec<_>>()
        .join("\n");
    read(&mut progress, &output, 2);
    let steps = &progress.report.parts[0].steps;
    assert_eq!(steps.len(), STEP_LIMIT);
    // The last step kept was closed by the next one, though that one is not kept.
    assert_eq!(steps.last().unwrap().finished_at, Some(2));
    let summary = progress.summary().unwrap()["script"].clone();
    assert_eq!(summary["stage"], 1000);
    assert_eq!(summary["percent"], 99);
}

#[test]
fn only_a_change_is_saved_and_the_report_outlives_the_progress() {
    let app = crate::tests::app(false);
    let job = crate::model::Job {
        id: "setup".into(),
        kind: "create".into(),
        resource: None,
        name: None,
        status: "running".into(),
        created_at: 1,
        finished_at: None,
        log: String::new(),
        result: None,
        progress: None,
        stages: Vec::new(),
        script: None,
    };
    app.db.put("job", "setup", &job).unwrap();
    let mut progress = ScriptProgress::setup(vec![part("setup", "Setup", "sleep 1\n")]);
    progress.start_part(0, 1);
    app.log_script_output(
        "setup",
        "THORIC_STAGE=1/2 Waiting\nworking\nTHORIC_LINE=1\n",
        &mut progress,
    )
    .unwrap();
    let saved: crate::model::Job = app.db.get("job", "setup").unwrap();
    assert_eq!(saved.log, "THORIC_STAGE=1/2 Waiting\nworking\n");
    assert_eq!(saved.progress.unwrap()["script"]["label"], "Waiting");
    assert_eq!(saved.script.unwrap().parts[0].steps.len(), 1);
    // Nothing changed, so nothing is written over another writer's record.
    let mut moved: crate::model::Job = app.db.get("job", "setup").unwrap();
    moved.progress = None;
    app.db.put("job", "setup", &moved).unwrap();
    app.save_script("setup", &mut progress).unwrap();
    assert_eq!(
        app.db
            .get::<crate::model::Job>("job", "setup")
            .unwrap()
            .progress,
        None
    );
    progress.finish_part(0, 2);
    progress.end();
    app.save_script("setup", &mut progress).unwrap();
    let saved: crate::model::Job = app.db.get("job", "setup").unwrap();
    assert_eq!(saved.progress, None);
    assert_eq!(saved.script.unwrap().parts[0].finished_at, Some(2));
    // Job lists leave the report out, as they leave the log out.
    assert!(app.db.jobs_summary().unwrap()[0].script.is_none());
}

/// Template authors and agents learn the markers and the fields from the guides, so a marker or
/// a field renamed here must be renamed there.
#[test]
fn the_markers_and_the_fields_are_documented() {
    let fields = [
        "`THORIC_STAGE=",
        "`THORIC_SKIP=",
        "`part`",
        "`parts`",
        "`stage`",
        "`stages`",
        "`label`",
        "`line`",
        "`lines`",
        "`percent`",
        "`skipped`",
        "`cached`",
        "`steps`",
    ];
    for (path, guide) in [
        ("docs/API.md", include_str!("../../../docs/API.md")),
        ("docs/MCP.md", include_str!("../../../docs/MCP.md")),
    ] {
        for field in fields {
            assert!(guide.contains(field), "{path} must document {field}");
        }
    }
    for (path, guide) in [
        ("docs/USAGE.md", include_str!("../../../docs/USAGE.md")),
        (
            "docs/DEVELOPMENT.md",
            include_str!("../../../docs/DEVELOPMENT.md"),
        ),
    ] {
        for marker in ["THORIC_STAGE=", "THORIC_SKIP="] {
            assert!(guide.contains(marker), "{path} must show {marker}");
        }
    }
}
