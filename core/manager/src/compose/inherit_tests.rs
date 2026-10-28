//! Pins down what an image built on a saved image keeps of that image's compose: the base's
//! steps before its own, named as merged templates name them, a step installed again in place
//! of the base's, and nothing changed by adding the base's compose to an image that has it.
use super::*;
use crate::compose::{Parameter, parameters::Kind};
use std::collections::BTreeMap;

fn step(id: &str, script: &str, parameters: Vec<Parameter>) -> Step {
    Step {
        id: id.into(),
        name: String::new(),
        parameters,
        script: script.into(),
        root: false,
        timeout_seconds: 600,
    }
}

/// A compose of one step that names none, as a template's is.
fn flat(script: &str, parameters: Vec<Parameter>) -> Compose {
    Compose {
        steps: vec![step("", script, parameters)],
    }
}

/// The reverse engineering template's compose: a volume of the container's own for the analysis.
fn analysis() -> Compose {
    flat(
        "ghidra-project \"$ANALYSIS\"\n",
        vec![Parameter {
            name: "ANALYSIS".into(),
            kind: Kind::Volume,
            path: "/home/desktop/Analysis".into(),
            new_volume: true,
            ..Default::default()
        }],
    )
}

fn chrome() -> Compose {
    flat(
        "chrome-profile \"$PORT\"\n",
        vec![Parameter {
            name: "PORT".into(),
            default: "9222".into(),
            ..Default::default()
        }],
    )
}

fn image(id: &str, compose: Option<Compose>, templates: &[&str]) -> ImageRecord {
    ImageRecord {
        id: id.into(),
        name: id.into(),
        image_id: format!("sha256:{id}"),
        reference: format!("thoric/custom:{id}"),
        parent_image_id: None,
        source_container: None,
        kind: "build".into(),
        created_at: 1,
        description: String::new(),
        desktop: None,
        build: None,
        compose,
        icon: None,
        template_ids: templates.iter().map(|id| id.to_string()).collect(),
    }
}

/// An image built on `base`, as builds recorded it before they kept the base's compose.
fn built_on(
    base: &ImageRecord,
    id: &str,
    compose: Option<Compose>,
    templates: &[&str],
) -> ImageRecord {
    ImageRecord {
        parent_image_id: Some(base.image_id.clone()),
        ..image(id, compose, templates)
    }
}

fn ids(compose: &Compose) -> Vec<&str> {
    compose.steps.iter().map(|step| step.id.as_str()).collect()
}

#[test]
fn without_a_base_compose_an_image_keeps_its_own_and_without_its_own_takes_the_base_s() {
    let plain = image("desktop", None, &[]);
    assert_eq!(
        extend(None, Some(chrome()), &[], "Chrome").unwrap(),
        Some(chrome())
    );
    assert_eq!(
        extend(Some(&plain), Some(chrome()), &[], "Chrome").unwrap(),
        Some(chrome())
    );
    // A compose of one step stays one, so its values and volumes keep their names.
    let re = image("re", Some(analysis()), &["reverse-engineering"]);
    let kept = extend(Some(&re), None, &[], "Tools").unwrap().unwrap();
    assert_eq!(kept, analysis());
    assert!(serde_json::to_value(&kept).unwrap().get("steps").is_none());
}

#[test]
fn composes_of_one_step_become_steps_named_after_their_templates() {
    let re = image("re", Some(analysis()), &["reverse-engineering"]);
    let joined = extend(
        Some(&re),
        Some(chrome()),
        &["chrome".into()],
        "re and Chrome",
    )
    .unwrap()
    .unwrap();
    assert_eq!(ids(&joined), ["reverse-engineering", "chrome"]);
    // A step without a name of its own is called after the image it came from.
    assert_eq!(joined.steps[0].name, "re");
    assert_eq!(joined.steps[1].name, "re and Chrome");
    assert_eq!(joined.steps[0].script, analysis().steps[0].script);
    // Each step asks for its own values, by its step, and the analysis volume is still the
    // container's own.
    let values = joined.resolve(&BTreeMap::new(), "c1").unwrap();
    assert_eq!(
        values["reverse-engineering.ANALYSIS"],
        "thoric-c1-reverse-engineering-analysis"
    );
    assert_eq!(values["chrome.PORT"], "9222");
    let named = BTreeMap::from([("PORT".to_owned(), "9333".to_owned())]);
    assert_eq!(joined.resolve(&named, "c1").unwrap()["chrome.PORT"], "9333");
}

#[test]
fn without_templates_the_steps_are_base_and_build_and_a_chosen_id_replaces_nothing() {
    let a = image("a", Some(analysis()), &[]);
    let b_compose = extend(Some(&a), Some(chrome()), &[], "b").unwrap().unwrap();
    assert_eq!(ids(&b_compose), ["base", "build"]);
    // A third build with a compose of its own on the second keeps both of its steps.
    let b = built_on(&a, "b", Some(b_compose), &[]);
    let own = flat("echo third\n", Vec::new());
    let c = extend(Some(&b), Some(own), &[], "c").unwrap().unwrap();
    assert_eq!(ids(&c), ["base", "build", "build-2"]);
}

#[test]
fn a_template_installed_again_takes_the_place_of_its_step() {
    let re = image("re", Some(analysis()), &["reverse-engineering"]);
    let both = extend(Some(&re), Some(chrome()), &["chrome".into()], "both")
        .unwrap()
        .unwrap();
    let both = image("both", Some(both), &["reverse-engineering", "chrome"]);
    let newer = flat(
        "chrome-profile --newer \"$PORT\"\n",
        chrome().steps[0].parameters.clone(),
    );
    let again = extend(
        Some(&both),
        Some(newer.clone()),
        &["chrome".into()],
        "again",
    )
    .unwrap()
    .unwrap();
    assert_eq!(ids(&again), ["reverse-engineering", "chrome"]);
    assert_eq!(again.steps[1].script, newer.steps[0].script);
    // Over a base of one step, what is left is the build's own, as it gave it.
    let chrome_image = image("chrome", Some(chrome()), &["chrome"]);
    assert_eq!(
        extend(
            Some(&chrome_image),
            Some(newer.clone()),
            &["chrome".into()],
            "again"
        )
        .unwrap(),
        Some(newer)
    );
}

#[test]
fn adding_the_base_s_compose_to_an_image_that_has_it_changes_nothing() {
    let re = image("re", Some(analysis()), &["reverse-engineering"]);
    let once = extend(Some(&re), Some(chrome()), &["chrome".into()], "both")
        .unwrap()
        .unwrap();
    let twice = extend(
        Some(&re),
        Some(once.clone()),
        &["reverse-engineering".into(), "chrome".into()],
        "both",
    )
    .unwrap();
    assert_eq!(twice, Some(once));
    // A step copied by hand from the base counts as the base's, whatever its id.
    let a = image("a", Some(analysis()), &[]);
    let copied = Compose {
        steps: vec![
            Step {
                id: "analysis".into(),
                ..analysis().steps[0].clone()
            },
            Step {
                id: "browser".into(),
                ..chrome().steps[0].clone()
            },
        ],
    };
    assert_eq!(
        extend(Some(&a), Some(copied.clone()), &[], "b").unwrap(),
        Some(copied)
    );
}

#[test]
fn composes_that_do_not_fit_together_are_refused() {
    let many = |prefix: &str, count: usize| Compose {
        steps: (0..count)
            .map(|n| {
                step(
                    &format!("{prefix}{n}"),
                    &format!("echo {prefix}{n}\n"),
                    Vec::new(),
                )
            })
            .collect(),
    };
    let base = image("base", Some(many("a", 5)), &[]);
    assert!(extend(Some(&base), Some(many("b", 4)), &[], "nine").is_err());
    // Two steps cannot mount volumes at one path.
    let re = image("re", Some(analysis()), &["reverse-engineering"]);
    let other = flat("true\n", analysis().steps[0].parameters.clone());
    assert!(
        extend(Some(&re), Some(other), &["other".into()], "other")
            .unwrap_err()
            .to_string()
            .contains("/home/desktop/Analysis")
    );
}

#[test]
fn the_templates_of_both_are_kept_once_and_the_newest_within_the_limit() {
    let base = ["reverse-engineering".to_owned(), "chrome".to_owned()];
    assert_eq!(
        joined(&base, &["chrome".into(), "vscode".into()]),
        ["reverse-engineering", "chrome", "vscode"]
    );
    let eight = (0..8).map(|n| format!("t{n}")).collect::<Vec<_>>();
    let kept = joined(&eight, &["mine".into()]);
    assert_eq!(kept.len(), BUILD_PART_LIMIT);
    assert_eq!(kept.first().unwrap(), "t1");
    assert_eq!(kept.last().unwrap(), "mine");
}

#[test]
fn the_list_marks_a_build_that_lacks_steps_of_its_base_s_compose() {
    let re = image("re", Some(analysis()), &["reverse-engineering"]);
    let plain = image("plain", None, &[]);
    let dropped = built_on(&re, "dropped", Some(chrome()), &["chrome"]);
    let bare = built_on(&re, "bare", None, &[]);
    let complete = built_on(
        &re,
        "complete",
        extend(Some(&re), Some(chrome()), &["chrome".into()], "complete").unwrap(),
        &["reverse-engineering", "chrome"],
    );
    let on_plain = built_on(&plain, "on-plain", Some(chrome()), &["chrome"]);
    let snapshot = ImageRecord {
        kind: "snapshot".into(),
        ..built_on(&re, "snapshot", Some(chrome()), &[])
    };
    let records = [
        re.clone(),
        plain,
        dropped.clone(),
        bare.clone(),
        complete.clone(),
        on_plain.clone(),
        snapshot.clone(),
    ];
    assert!(base_compose_missing(&records, &dropped));
    assert!(base_compose_missing(&records, &bare));
    assert!(!base_compose_missing(&records, &complete));
    assert!(!base_compose_missing(&records, &on_plain));
    // A snapshot keeps the compose of the container it was taken from.
    assert!(!base_compose_missing(&records, &snapshot));
    assert!(!base_compose_missing(&records, &re));
}

#[tokio::test]
async fn an_image_that_lacks_its_base_s_compose_gets_it_once() {
    let app = crate::tests::app(false);
    let re = image("re", Some(analysis()), &["reverse-engineering"]);
    let dropped = built_on(&re, "dropped", Some(chrome()), &["chrome"]);
    for record in [&re, &dropped] {
        app.db.put("image", &record.id, record).unwrap();
    }
    let Json(summary) = add_base(State(app.clone()), Path("dropped".into()))
        .await
        .unwrap();
    assert_eq!(
        summary["template_ids"],
        serde_json::json!(["reverse-engineering", "chrome"])
    );
    let saved: ImageRecord = app.db.get("image", "dropped").unwrap();
    assert_eq!(
        ids(saved.compose.as_ref().unwrap()),
        ["reverse-engineering", "chrome"]
    );
    // A list leaves the scripts out.
    assert!(summary["compose"]["steps"][0].get("script").is_none());
    let again = add_base(State(app.clone()), Path("dropped".into()))
        .await
        .unwrap_err();
    assert_eq!(again.0, StatusCode::CONFLICT);
    // The base itself was built on nothing Thoric keeps.
    let alone = add_base(State(app), Path("re".into())).await.unwrap_err();
    assert_eq!(alone.0, StatusCode::CONFLICT);
}
