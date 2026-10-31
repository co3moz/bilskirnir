//! Pins down which wait conditions are taken and which are refused before any polling, and that a
//! round that could not look, for a busy screenshot slot or a step that ran out of time, keeps the
//! wait going until its deadline, which no round outlasts but a short wait's first look, and then
//! answers with the last observation and why the last round failed.
use super::*;
use crate::model::{CONTAINERS, Container};

const BUSY: &str = "Two screenshots are already being taken; retry in a moment";

fn unsatisfied(window: &str) -> Result<Observation> {
    Ok(Observation {
        result: json!({ "window": window }),
        satisfied: false,
    })
}

fn failed(status: StatusCode, message: &str) -> Result<Observation> {
    Err(Error(status, message.into()))
}

#[tokio::test(start_paused = true)]
async fn rounds_that_could_not_look_keep_the_wait_going_until_its_deadline() {
    let started = Instant::now();
    let mut rounds = 0;
    let answer = poll(Duration::from_secs(10), Duration::from_millis(500), |_| {
        rounds += 1;
        let outcome = match rounds {
            1 => unsatisfied("first"),
            2 => failed(StatusCode::GATEWAY_TIMEOUT, "Docker did not answer"),
            _ => failed(StatusCode::TOO_MANY_REQUESTS, BUSY),
        };
        async move { outcome }
    })
    .await
    .unwrap();
    assert_eq!(rounds, 20, "a round every interval until the deadline");
    assert_eq!(
        answer,
        json!({"window": "first", "satisfied": false, "elapsed_ms": 9500, "last_error": BUSY})
    );
    assert_eq!(started.elapsed(), Duration::from_millis(9500));
}

#[tokio::test(start_paused = true)]
async fn a_round_that_looks_again_ends_the_wait_or_clears_why_one_could_not() {
    let mut rounds = 0;
    let answer = poll(Duration::from_secs(10), Duration::from_millis(500), |_| {
        rounds += 1;
        let outcome = match rounds {
            1 | 2 => failed(StatusCode::TOO_MANY_REQUESTS, BUSY),
            _ => Ok(Observation {
                result: json!({"window": {"title": "Game"}}),
                satisfied: true,
            }),
        };
        async move { outcome }
    })
    .await
    .unwrap();
    assert_eq!(
        answer,
        json!({"window": {"title": "Game"}, "satisfied": true, "elapsed_ms": 1000})
    );
    rounds = 0;
    let answer = poll(Duration::from_secs(2), Duration::from_millis(500), |_| {
        rounds += 1;
        let outcome = match rounds {
            1 => failed(StatusCode::TOO_MANY_REQUESTS, BUSY),
            _ => unsatisfied("later"),
        };
        async move { outcome }
    })
    .await
    .unwrap();
    assert_eq!(
        answer,
        json!({"window": "later", "satisfied": false, "elapsed_ms": 1500})
    );
}

#[tokio::test(start_paused = true)]
async fn any_other_error_ends_the_wait_at_once() {
    let mut rounds = 0;
    let error = poll(Duration::from_secs(10), Duration::from_millis(500), |_| {
        rounds += 1;
        async { failed(StatusCode::CONFLICT, "Start the container first") }
    })
    .await
    .unwrap_err();
    assert_eq!((error.0, rounds), (StatusCode::CONFLICT, 1));
}

#[tokio::test(start_paused = true)]
async fn no_round_outlasts_the_deadline_but_a_short_waits_first_look() {
    let started = Instant::now();
    let mut ends = Vec::new();
    let answer = poll(
        Duration::from_secs(10),
        Duration::from_millis(500),
        |until| {
            ends.push(until);
            by(until, "taking a screenshot", std::future::pending())
        },
    )
    .await
    .unwrap();
    assert_eq!(ends, [started + Duration::from_secs(10)]);
    assert_eq!(
        answer,
        json!({
            "satisfied": false,
            "elapsed_ms": 10_000,
            "last_error": "The wait ran out of time while taking a screenshot",
        })
    );
    // A wait shorter than a look still looks once.
    let started = Instant::now();
    let answer = poll(
        Duration::from_millis(100),
        Duration::from_millis(100),
        |until| {
            assert_eq!(until, started + FIRST_LOOK);
            by(until, "taking a screenshot", async {
                tokio::time::sleep(Duration::from_secs(2)).await;
                unsatisfied("slow")
            })
        },
    )
    .await
    .unwrap();
    assert_eq!(
        answer,
        json!({"window": "slow", "satisfied": false, "elapsed_ms": 2000})
    );
}

#[tokio::test(start_paused = true)]
async fn a_wait_whose_screenshots_stay_busy_answers_on_time_with_why() {
    let app = crate::test_support::app(false);
    let record = Container {
        id: "busy".into(),
        container: "thoric-busy".into(),
        private_network: "thoric-busy-private".into(),
        egress_network: "thoric-busy-egress".into(),
        source_image_id: "sha256:desktop".into(),
        created_at: 1,
        spec: crate::test_support::spec(),
        last_error: None,
        desktop: None,
        working_image: None,
        setup: None,
    };
    app.db.put(CONTAINERS, "busy", &record).unwrap();
    // Both slots stay taken by screenshots of other desktops.
    let _taken = app.screenshots.acquire_many(2).await.unwrap();
    let body: WaitFor =
        serde_json::from_value(json!({"screen_stable_ms": 1000, "timeout_ms": 12_000})).unwrap();
    let started = Instant::now();
    let answer = wait_for_condition(&app, "busy", &body).await.unwrap();
    assert_eq!(started.elapsed(), Duration::from_secs(12));
    assert_eq!(
        answer,
        json!({
            "satisfied": false,
            "elapsed_ms": 12_000,
            "last_error": "The wait ran out of time while taking a screenshot",
        })
    );
    // A wait that ends between two rounds names the slots the last one waited for.
    let body: WaitFor =
        serde_json::from_value(json!({"screen_changed": true, "timeout_ms": 5_400})).unwrap();
    let answer = wait_for_condition(&app, "busy", &body).await.unwrap();
    assert_eq!(answer["last_error"], BUSY);
}

#[test]
fn wait_conditions_are_validated() {
    let check =
        |value: serde_json::Value| serde_json::from_value::<WaitFor>(value).unwrap().validate();
    assert!(check(serde_json::json!({"window_title":"Game"})).is_ok());
    assert!(
        check(serde_json::json!({"application_id":"a","log_contains":"ready","timeout_ms":60000}))
            .is_ok()
    );
    assert!(check(serde_json::json!({"screen_stable_ms":1000,"region":"0,0,100,100"})).is_ok());
    // The screen can be waited on to change, and text to appear or go, in a region or not.
    assert!(check(serde_json::json!({"screen_changed":true})).is_ok());
    assert!(
        check(serde_json::json!({"screen_changed":true,"screen_change_percent":0.01,"region":"0,0,100,100"}))
            .is_ok()
    );
    assert!(
        check(serde_json::json!({"text":"Play","text_state":"gone","region":"0,0,1,1"})).is_ok()
    );
    for bad in [
        serde_json::json!({}),
        serde_json::json!({"application_state":"running"}),
        serde_json::json!({"application_id":"a","application_state":"paused"}),
        serde_json::json!({"log_contains":"x"}),
        serde_json::json!({"window_title":"x","timeout_ms":60001}),
        serde_json::json!({"screen_stable_ms":20000,"timeout_ms":10000}),
        serde_json::json!({"window_title":"x","region":"0,0,1,1"}),
        serde_json::json!({"screen_changed":false}),
        serde_json::json!({"window_title":"x","screen_change_percent":5.0}),
        serde_json::json!({"screen_changed":true,"screen_change_percent":0.0}),
        serde_json::json!({"screen_changed":true,"screen_change_percent":100.5}),
        serde_json::json!({"text":"   "}),
        serde_json::json!({"text":"x".repeat(257)}),
        serde_json::json!({"window_title":"x","text_state":"gone"}),
        serde_json::json!({"text":"Play","text_state":"hidden"}),
    ] {
        assert!(check(bad).is_err());
    }
}
