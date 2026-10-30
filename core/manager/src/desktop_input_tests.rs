//! Pins down that an input request waits for a container held by a job without taking one of the
//! input slots every container shares, and gives up with a busy answer in bounded time, while a
//! script that shares the container lets it through and another input request makes it wait; that
//! relative motion is planned as one movement whose steps add up to its offset; and that sending a
//! plan moves the pointer relatively through XTest in order with the VNC input, presses and
//! releases buttons where that motion left the pointer, and emulates it when XTest cannot run. Also
//! which file paths and input actions are refused, how keys and buttons are planned, and that those
//! held at the end of a request are released.
use super::*;
use crate::files;

#[tokio::test]
async fn a_held_container_answers_busy_without_taking_an_input_slot() {
    let app = crate::test_support::app(false);
    let lock = app.resource_lock("container:held");
    let _job = lock.lock().await;
    let waiting = tokio::spawn({
        let app = app.clone();
        async move { perform(&app, "held", &[]).await }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    // Every slot stays free for the other containers while this request waits.
    assert_eq!(app.desktop_inputs.available_permits(), 4);
    let error = tokio::time::timeout(LOCK_WAIT + Duration::from_secs(2), waiting)
        .await
        .expect("the wait is bounded")
        .unwrap()
        .unwrap_err();
    assert_eq!(error.0, StatusCode::CONFLICT);
    assert_eq!(app.desktop_inputs.available_permits(), 4);
}

#[tokio::test]
async fn input_shares_the_container_with_a_script_and_takes_turns_with_other_input() {
    let app = crate::test_support::app(false);
    let _script = app.resource_lock("container:busy").share_owned().await;
    // Past the lock: the container that is not there is what fails it.
    let error = perform(&app, "busy", &[]).await.unwrap_err();
    assert_eq!(error.0, StatusCode::NOT_FOUND, "{}", error.1);
    let _other = app.resource_lock("input:busy").lock_owned().await;
    let error = tokio::time::timeout(
        LOCK_WAIT + Duration::from_secs(2),
        perform(&app, "busy", &[]),
    )
    .await
    .expect("the wait is bounded")
    .unwrap_err();
    assert_eq!(error.0, StatusCode::CONFLICT);
}

fn actions(value: serde_json::Value) -> Vec<Action> {
    serde_json::from_value::<Input>(json!({ "actions": value }))
        .unwrap()
        .actions
}

#[test]
fn relative_motion_is_one_movement_whose_steps_add_up_to_its_offset() {
    let planned = plan(
        &actions(json!([
            {"type":"move","x":795,"y":5},
            {"type":"move_relative","dx":10,"dy":-7,"steps":3,"duration_ms":90},
            {"type":"button_down"},
        ])),
        800,
        600,
        None,
    )
    .unwrap();
    let Step::Relative(motion) = &planned.steps[1] else {
        panic!("{:?}", planned.steps);
    };
    assert_eq!((motion.from, motion.pause, motion.mask), ((795, 5), 30, 0));
    assert_eq!(motion.moves(), [(3, -2), (3, -2), (4, -3)]);
    // Emulated, the pointer stops at the edge; the plan expects it there.
    assert_eq!(
        motion.emulated((795, 5), 800, 600),
        [(798, 3), (799, 1), (799, 0)]
    );
    assert_eq!(
        planned.steps[2],
        Step::Buttons {
            mask: 1,
            expected: (799, 0)
        }
    );
    assert_eq!((planned.pointer, planned.delay_ms), (Some((799, 0)), 90));
    assert_eq!(pointer(1, 20, 30), [5, 1, 0, 20, 0, 30]);
}

/// A VNC server that answers fences and notes each pointer event and fence it is sent.
#[cfg(unix)]
async fn vnc_server() -> (TcpStream, tokio::task::JoinHandle<Vec<String>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let (mut server, _) = listener.accept().await.unwrap();
    let noted = tokio::spawn(async move {
        let mut noted = Vec::new();
        while let Ok(kind) = server.read_u8().await {
            match kind {
                4 => {
                    let mut key = [0; 7];
                    server.read_exact(&mut key).await.unwrap();
                }
                5 => {
                    let mask = server.read_u8().await.unwrap();
                    let (x, y) = (
                        server.read_u16().await.unwrap(),
                        server.read_u16().await.unwrap(),
                    );
                    noted.push(format!("pointer {mask} {x},{y}"));
                }
                248 => {
                    let mut padding = [0; 3];
                    server.read_exact(&mut padding).await.unwrap();
                    let flags = server.read_u32().await.unwrap();
                    let mut data = vec![0; usize::from(server.read_u8().await.unwrap())];
                    server.read_exact(&mut data).await.unwrap();
                    noted.push("fence".into());
                    let mut reply = vec![248, 0, 0, 0];
                    reply.extend((flags & 3).to_be_bytes());
                    reply.push(data.len() as u8);
                    reply.extend(data);
                    server.write_all(&reply).await.unwrap();
                }
                other => panic!("unexpected client message {other}"),
            }
        }
        noted
    });
    (client, noted)
}

/// A desktop whose Docker CLI is a shell script: it owns the container, and its xdotool reports
/// the pointer at 640,100 unless a file named `broken` makes it missing.
#[cfg(unix)]
fn desktop() -> (
    std::sync::Arc<App>,
    crate::model::Container,
    std::path::PathBuf,
) {
    let dir = std::env::temp_dir().join(format!("thoric-input-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    let docker = dir.join("docker");
    let script = r#"#!/bin/sh
cd 'DIR'
echo "$*" >> calls
case "$1 $2" in
  'container inspect') printf '[{"Id":"desktop-id","Config":{"Labels":{"thoric.managed":"true","thoric.environment":"c1"}}}]' ;;
  'exec --user')
    if [ -f broken ]; then echo 'exec: "xdotool": executable file not found in $PATH' >&2; exit 127; fi
    printf 'X=640\nY=100\nSCREEN=0\nWINDOW=4\n' ;;
  *) exit 1 ;;
esac
"#;
    crate::test_support::write_script(&docker, &script.replace("DIR", &dir.display().to_string()));
    let mut app = crate::test_support::app(false);
    std::sync::Arc::get_mut(&mut app).unwrap().config.docker_bin = docker.display().to_string();
    let env = serde_json::from_value(json!({
        "id": "c1", "container": "thoric-c1", "private_network": "thoric-c1-private",
        "egress_network": "thoric-c1-egress", "source_image_id": "sha256:desktop", "created_at": 1,
        "spec": {"name": "Game", "image": "example/desktop:1"},
    }))
    .unwrap();
    (app, env, dir)
}

/// Press the left button at 100,100, turn by 50 to the right in two steps and release the button
/// wherever the turn left the pointer.
#[cfg(unix)]
fn turn() -> Planned {
    plan(
        &actions(json!([
            {"type":"button_down","x":100,"y":100},
            {"type":"move_relative","dx":50,"dy":0,"steps":2,"duration_ms":20},
            {"type":"button_up"},
        ])),
        800,
        600,
        None,
    )
    .unwrap()
}

#[cfg(unix)]
#[tokio::test]
async fn relative_motion_goes_through_xtest_between_the_vnc_input_around_it() {
    let (app, env, dir) = desktop();
    let (mut stream, noted) = vnc_server().await;
    let sent = send(&app, &env, &mut stream, &turn(), (800, 600))
        .await
        .unwrap();
    stream.shutdown().await.unwrap();
    // The button goes down over VNC, the server confirms it before the motion begins, and the
    // button comes up where xdotool says the pointer is, not where the plan expected it.
    assert_eq!(
        noted.await.unwrap(),
        ["pointer 1 100,100", "fence", "pointer 0 640,100"]
    );
    assert_eq!((sent.here, sent.emulated), (Some((640, 100)), None));
    let calls = std::fs::read_to_string(dir.join("calls")).unwrap();
    assert!(
        calls.contains("exec --user desktop --env DISPLAY=:0 desktop-id xdotool mousemove_relative -- 25 0 sleep 0.010 mousemove_relative -- 25 0 sleep 0.010 getmouselocation --shell"),
        "{calls}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn relative_motion_is_emulated_with_positions_when_xtest_cannot_move_the_pointer() {
    let (app, env, dir) = desktop();
    std::fs::write(dir.join("broken"), "").unwrap();
    let (mut stream, noted) = vnc_server().await;
    let sent = send(&app, &env, &mut stream, &turn(), (800, 600))
        .await
        .unwrap();
    stream.shutdown().await.unwrap();
    assert_eq!(
        noted.await.unwrap(),
        [
            "pointer 1 100,100",
            "fence",
            "pointer 1 125,100",
            "pointer 1 150,100",
            "pointer 0 150,100"
        ]
    );
    let reason = sent.emulated.unwrap();
    assert!(reason.contains("executable file not found"), "{reason}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn file_paths_and_vnc_actions_reject_invalid_requests() {
    for path in [
        "",
        "/",
        "relative.txt",
        "/home/desktop/../root/file",
        "/home//file",
        "/home/file/",
        "/home/./file",
        "/home/file\0",
    ] {
        assert!(files::split_path(path).is_err(), "{path}");
    }
    assert_eq!(
        files::split_path("/etc/os-release").unwrap(),
        ("/etc", "os-release")
    );
    for body in [
        serde_json::json!({"actions":[]}),
        serde_json::json!({"actions":[{"type":"click","x":800,"y":0}]}),
        serde_json::json!({"actions":[{"type":"click","x":0,"y":0,"count":0}]}),
        serde_json::json!({"actions":[{"type":"drag","from_x":1,"from_y":1,"to_x":2,"to_y":2,"duration_ms":99999}]}),
        serde_json::json!({"actions":[{"type":"key_press","keys":["Control","Ctrl"]}]}),
        serde_json::json!({"actions":[{"type":"wait","milliseconds":20000}]}),
        serde_json::json!({"actions":[{"type":"type_text","text":"\0"}]}),
    ] {
        let input: Input = serde_json::from_value(body).unwrap();
        assert!(plan(&input.actions, 800, 600, None).is_err());
    }
    assert_eq!(keysym("Control").unwrap(), 0xffe3);
    assert_eq!(keysym("Ω").unwrap(), 0x010003a9);
    assert_eq!(keysym("\n").unwrap(), 0xff0d);
    assert!(keysym("F13").is_err());
    let input: Input = serde_json::from_value(serde_json::json!({"actions":[{"type":"key_press","keys":["Control","a"]},{"type":"click","x":20,"y":30}]})).unwrap();
    let plan = plan(&input.actions, 800, 600, None).unwrap().steps;
    assert_eq!(
        plan[0],
        Step::Keys(
            [
                vec![4, 1, 0, 0, 0, 0, 255, 227],
                vec![4, 1, 0, 0, 0, 0, 0, 97],
                vec![4, 0, 0, 0, 0, 0, 0, 97],
                vec![4, 0, 0, 0, 0, 0, 255, 227]
            ]
            .concat()
        )
    );
    assert_eq!(
        plan[1],
        Step::Pointer {
            mask: 1,
            x: 20,
            y: 30,
            pause: 40
        }
    );
    assert_eq!(
        plan[2],
        Step::Pointer {
            mask: 0,
            x: 20,
            y: 30,
            pause: 80
        }
    );
}

#[test]
fn held_inputs_are_tracked_and_released_at_the_end_of_a_request() {
    let input: Input = serde_json::from_value(serde_json::json!({"actions":[
        {"type":"key_down","keys":["w","Shift"]},
        {"type":"button_down","x":10,"y":20,"button":"right"},
        {"type":"move_relative","dx":15,"dy":-30},
        {"type":"wait","milliseconds":1500},
        {"type":"key_up","keys":["Shift"]}
    ]}))
    .unwrap();
    let planned = plan(&input.actions, 800, 600, None).unwrap();
    assert_eq!(planned.released_keys, vec!["w"]);
    assert_eq!(planned.released_buttons, vec!["right"]);
    assert_eq!(planned.pointer, Some((25, 0)));
    assert_eq!(planned.delay_ms, 1500);
    // The relative move keeps the right button pressed, and the plan expects the pointer to stop
    // at the desktop's edge, where the button is released.
    assert_eq!(
        planned.steps[1],
        Step::Pointer {
            mask: 4,
            x: 10,
            y: 20,
            pause: 0
        }
    );
    assert_eq!(
        planned.steps[2],
        Step::Relative(Relative {
            dx: 15,
            dy: -30,
            steps: 1,
            pause: 0,
            mask: 4,
            from: (10, 20)
        })
    );
    assert_eq!(
        planned.steps.last().unwrap(),
        &Step::Buttons {
            mask: 0,
            expected: (25, 0)
        }
    );
    assert!(!needs_start_position(&input.actions));
    let relative: Input = serde_json::from_value(
        serde_json::json!({"actions":[{"type":"move_relative","dx":1,"dy":1}]}),
    )
    .unwrap();
    assert!(needs_start_position(&relative.actions));
    assert!(plan(&relative.actions, 800, 600, None).is_err());
    assert_eq!(
        plan(&relative.actions, 800, 600, Some((799, 599)))
            .unwrap()
            .pointer,
        Some((799, 599))
    );
    for body in [
        serde_json::json!({"actions":[{"type":"hold_keys","keys":["w"],"milliseconds":10001}]}),
        serde_json::json!({"actions":(0..4).map(|_| serde_json::json!({"type":"wait","milliseconds":10000})).collect::<Vec<_>>()}),
        serde_json::json!({"actions":[{"type":"button_down","x":5}]}),
        serde_json::json!({"actions":[{"type":"button_up"}]}),
    ] {
        let input: Input = serde_json::from_value(body).unwrap();
        assert!(plan(&input.actions, 800, 600, None).is_err());
    }
}
