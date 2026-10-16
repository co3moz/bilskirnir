//! Pins down how a script build reports where it is: stages only move forward and close the one
//! before them, BuildKit's own progress lines start the layer and image stages while the script's
//! output never does, and the log names each stage on a line of its own.
use super::*;
use crate::model::Job;

fn job(stages: bool) -> Job {
    let mut job = Job {
        id: "build".into(),
        kind: "build".into(),
        resource: None,
        status: "running".into(),
        created_at: 1,
        finished_at: None,
        log: String::new(),
        result: None,
        progress: None,
        stages: Vec::new(),
    };
    if stages {
        job.stages = BUILD_STAGES
            .iter()
            .map(|(name, _)| Stage {
                name: (*name).into(),
                started_at: None,
                finished_at: None,
            })
            .collect();
    }
    job
}

fn times(job: &Job) -> Vec<(&str, Option<u64>, Option<u64>)> {
    job.stages
        .iter()
        .map(|stage| (stage.name.as_str(), stage.started_at, stage.finished_at))
        .collect()
}

#[test]
fn stages_move_forward_and_finish_the_running_one() {
    let mut job = job(true);
    assert_eq!(job.begin_stage("base", 10), Some(0));
    assert_eq!(job.begin_stage("script", 12), Some(1));
    // A repeated marker, or one for a stage already passed, changes nothing.
    assert_eq!(job.begin_stage("script", 13), None);
    assert_eq!(job.begin_stage("base", 14), None);
    assert_eq!(job.begin_stage("unknown", 14), None);
    // Without a marker for it, the layer stage is skipped rather than given made-up times.
    assert_eq!(job.begin_stage("image", 20), Some(3));
    assert_eq!(job.begin_stage("layer", 21), None);
    job.finish_stages(25);
    assert_eq!(
        times(&job),
        [
            ("base", Some(10), Some(12)),
            ("script", Some(12), Some(20)),
            ("layer", None, None),
            ("image", Some(20), Some(25)),
            ("cleanup", None, None),
        ]
    );
    // A stage that never started carries only its name.
    let value = serde_json::to_value(&job).unwrap();
    assert_eq!(value["stages"][4], serde_json::json!({"name": "cleanup"}));
    assert_eq!(
        value["stages"][0],
        serde_json::json!({"name": "base", "started_at": 10, "finished_at": 12})
    );
    // Other jobs keep their shape.
    assert!(serde_json::to_value(self::job(false)).unwrap()["stages"].is_null());
}

/// `docker build --progress=plain` output as BuildKit writes it, with the classic image store
/// and then with the containerd image store.
const CLASSIC: &str = "#0 building with \"default\" instance using docker driver

#1 [internal] load build definition from Dockerfile
#1 transferring dockerfile: 247B done
#1 DONE 0.0s

#4 [1/3] FROM docker.io/bilskirnir/build-base:0f3a
#4 DONE 0.1s

#6 [2/3] COPY setup.sh /tmp/wsld-setup.sh
#6 DONE 0.1s

#7 [3/3] RUN bash /tmp/wsld-setup.sh && rm /tmp/wsld-setup.sh
#7 0.231 exporting to image
#7 0.232 #9 naming to nowhere
#7 2.004 Setting up python3 (3.12.3-0ubuntu2) ...
#7 DONE 250.2s

#8 exporting to image
#8 exporting layers
#8 exporting layers 171.4s done
#8 writing image sha256:5f1d2c done
#8 naming to docker.io/bilskirnir/custom:0f3a done
#8 DONE 171.6s
";
const CONTAINERD: &str = "#8 exporting to image
#8 exporting layers 12.3s done
#8 exporting manifest sha256:aa done
#8 exporting config sha256:bb done
#8 exporting attestation manifest sha256:cc done
#8 exporting manifest list sha256:dd done
#8 naming to docker.io/bilskirnir/custom:0f3a done
#8 unpacking to docker.io/bilskirnir/custom:0f3a 4.1s done
#8 DONE 16.5s
";

#[test]
fn buildkit_progress_lines_start_the_layer_and_image_stages() {
    let marked = |text: &str| {
        text.lines()
            .filter_map(|line| buildkit_stage(line).map(|stage| (line.to_owned(), stage)))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        marked(CLASSIC),
        [
            ("#8 exporting to image".into(), "layer"),
            ("#8 exporting layers".into(), "layer"),
            ("#8 exporting layers 171.4s done".into(), "layer"),
            ("#8 writing image sha256:5f1d2c done".into(), "image"),
            (
                "#8 naming to docker.io/bilskirnir/custom:0f3a done".into(),
                "image"
            ),
        ]
    );
    let stages = marked(CONTAINERD)
        .into_iter()
        .map(|(_, stage)| stage)
        .collect::<Vec<_>>();
    assert_eq!(
        stages,
        [
            "layer", "layer", "image", "image", "image", "image", "image", "image"
        ]
    );
    // The legacy builder's steps and anything that is not a step line start nothing.
    for line in [
        "Step 4/6 : RUN bash /tmp/wsld-setup.sh && rm /tmp/wsld-setup.sh",
        "exporting to image",
        "# exporting to image",
        "#x exporting to image",
        "Stage 3/5: Saving the image layer",
    ] {
        assert_eq!(buildkit_stage(line), None, "{line}");
    }
}

#[test]
fn lines_wait_for_their_end() {
    let mut lines = Lines::default();
    assert_eq!(lines.push("#8 export"), "");
    assert_eq!(
        lines.push("ing to image\n#8 exp"),
        "#8 exporting to image\n"
    );
    assert_eq!(lines.push(""), "");
    assert_eq!(lines.finish(), "#8 exp");
    assert_eq!(lines.finish(), "");
    // A line that never ends is logged once it is too long to be a marker.
    let long = "x".repeat(LINE_LIMIT + 1);
    assert_eq!(lines.push(&long), long);
}

#[test]
fn build_output_names_each_stage_on_its_own_log_line() {
    let app = crate::tests::app(false);
    app.db.put("job", "build", &job(false)).unwrap();
    app.plan_stages("build", &BUILD_STAGES).unwrap();
    app.begin_stage("build", "base").unwrap();
    let download = serde_json::json!({"current": 1, "total": 2, "layers": 1, "layers_done": 0});
    app.set_progress("build", Some(download)).unwrap();
    app.begin_stage("build", "script").unwrap();
    // Output arrives in pieces that split lines, the marker line included.
    let mut lines = Lines::default();
    let mut rest = CLASSIC;
    while !rest.is_empty() {
        let (piece, after) = rest.split_at(rest.len().min(37));
        app.log_build_output("build", &lines.push(piece)).unwrap();
        rest = after;
    }
    app.log_build_output("build", &lines.finish()).unwrap();
    let job: Job = app.db.get("job", "build").unwrap();
    let names = job
        .stages
        .iter()
        .map(|stage| (stage.name.as_str(), stage.started_at.is_some()))
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            ("base", true),
            ("script", true),
            ("layer", true),
            ("image", true),
            ("cleanup", false)
        ]
    );
    // The download's progress ended with the stage it belonged to.
    assert_eq!(job.progress, None);
    let log = job.log.lines().collect::<Vec<_>>();
    assert_eq!(log[0], "Stage 1/5: Preparing the base image");
    assert_eq!(log[1], "Stage 2/5: Running the setup script");
    let layer = log
        .iter()
        .position(|line| *line == "Stage 3/5: Saving the image layer")
        .unwrap();
    assert_eq!(log[layer + 1], "#8 exporting to image");
    let image = log
        .iter()
        .position(|line| *line == "Stage 4/5: Writing and tagging the image")
        .unwrap();
    assert_eq!(log[image + 1], "#8 writing image sha256:5f1d2c done");
    // Every line BuildKit wrote is in the log, whole and in order.
    let output = log
        .iter()
        .filter(|line| !line.starts_with("Stage "))
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(output, CLASSIC.lines().collect::<Vec<_>>());
}

/// The dashboard names each stage from its catalog and the guides list them, so a stage added here
/// without a name there would show as its bare identifier.
#[test]
fn every_build_stage_is_named_in_the_interface_and_documented() {
    let catalog = include_str!("../web/src/locales/en.ts");
    for (name, _) in BUILD_STAGES {
        assert!(
            catalog.contains(&format!("\"stage.{name}\":")),
            "web/src/locales/en.ts must name the stage {name}"
        );
        for (path, guide) in [
            ("docs/API.md", include_str!("../docs/API.md")),
            ("docs/MCP.md", include_str!("../docs/MCP.md")),
        ] {
            assert!(
                guide.contains(&format!("`{name}`")),
                "{path} must document the stage {name}"
            );
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_build_job_reports_its_stages_and_where_it_failed() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!(
        "bilskirnir-stages-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    // A Docker CLI stand-in: images are desktop images, and a build writes the prepared output.
    let docker = dir.join("docker");
    std::fs::write(
        &docker,
        format!(
            "#!/bin/sh\ncd '{dir}'\ncase \"$1\" in\n  build) cat build-output >&2; exit $(cat build-status) ;;\nesac\ncase \"$1 $2\" in\n  'image inspect') echo '[{{\"Id\":\"sha256:feed\",\"Config\":{{\"Labels\":{{\"io.wsl-desktop.desktop\":\"true\"}}}}}}]' ;;\nesac\n",
            dir = dir.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut app = crate::tests::app(false);
    let config = &mut std::sync::Arc::get_mut(&mut app).unwrap().config;
    config.docker_bin = docker.display().to_string();
    config.data = dir.join("data");
    let run = |output: &str, status: u8| {
        std::fs::write(dir.join("build-output"), output).unwrap();
        std::fs::write(dir.join("build-status"), status.to_string()).unwrap();
        let app = app.clone();
        async move {
            let body = crate::api::BuildImage {
                name: "Staged".into(),
                base_image: "example/desktop:1".into(),
                internet: false,
                script: "true\n".into(),
                compose: None,
            };
            let started = crate::api::build(axum::extract::State(app.clone()), axum::Json(body))
                .await
                .unwrap();
            let id = started.0["job_id"].as_str().unwrap().to_owned();
            loop {
                let job: Job = app.db.get("job", &id).unwrap();
                if job.finished_at.is_some() {
                    break job;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }
    };

    let built = run(CLASSIC, 0).await;
    assert_eq!(built.status, "completed", "{}", built.log);
    for stage in &built.stages {
        let (Some(started), Some(finished)) = (stage.started_at, stage.finished_at) else {
            panic!("{} did not run: {:?}", stage.name, built.stages);
        };
        assert!(started <= finished && finished <= built.finished_at.unwrap());
    }
    assert!(built.log.contains("Stage 5/5: Cleaning up\n"));

    // A failed script leaves its own stage as the last one started; cleanup still runs unlisted.
    let failed = run(
        "#7 [3/3] RUN bash /tmp/wsld-setup.sh && rm /tmp/wsld-setup.sh\n#7 0.1 nope\n#7 ERROR: exit code: 17\n",
        1,
    )
    .await;
    assert_eq!(failed.status, "failed");
    assert_eq!(
        failed
            .stages
            .iter()
            .map(|stage| (stage.name.as_str(), stage.started_at.is_some()))
            .collect::<Vec<_>>(),
        [
            ("base", true),
            ("script", true),
            ("layer", false),
            ("image", false),
            ("cleanup", false)
        ]
    );
    assert!(failed.stages[1].finished_at.is_some());
    assert!(!dir.join("data/builds").join(&failed.id).exists());
    std::fs::remove_dir_all(&dir).unwrap();
}
