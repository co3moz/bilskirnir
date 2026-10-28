//! Pins down which of Docker's events name a list, that the daemon is asked for no more than
//! those, that a burst is published as one `changed` event, and, against a fake Engine API on a
//! Unix socket, that the stream says when it flows and is opened again after Docker restarts.
use super::*;
use tokio::sync::broadcast;

fn container(action: &str, managed: bool) -> Value {
    let mut attributes = json!({"name": "thoric-a", "image": "thoric/ubuntu-xfce:24.04"});
    if managed {
        attributes[LABEL] = json!("true");
    }
    json!({"Type": "container", "Action": action, "Actor": {"ID": "abc", "Attributes": attributes}})
}

#[test]
fn only_what_a_list_shows_names_it() {
    for action in ["start", "die", "destroy", "health_status: unhealthy"] {
        assert_eq!(
            changed_list(&container(action, true)),
            Some("containers"),
            "{action}"
        );
    }
    // Another project's container, and what every script and start send, change nothing shown.
    assert_eq!(changed_list(&container("start", false)), None);
    for action in ["exec_start: sh -c true", "attach", "kill", "top"] {
        assert_eq!(changed_list(&container(action, true)), None, "{action}");
    }
    let image =
        |action: &str| json!({"Type": "image", "Action": action, "Actor": {"ID": "sha256:1"}});
    assert_eq!(changed_list(&image("pull")), Some("images"));
    assert_eq!(changed_list(&image("delete")), Some("images"));
    assert_eq!(changed_list(&image("push")), None);
    let volume = json!({"Type": "volume", "Action": "destroy", "Actor": {"ID": "data"}});
    assert_eq!(changed_list(&volume), Some("volumes"));
    let network = json!({"Type": "network", "Action": "connect", "Actor": {"ID": "n"}});
    assert_eq!(changed_list(&network), None);
    assert_eq!(changed_list(&json!({"Type": "container"})), None);
}

#[test]
fn the_daemon_is_asked_only_for_the_watched_events() {
    let path = path();
    let query = path.strip_prefix("/events?filters=").unwrap();
    let filters: Value = serde_json::from_str(&percent_decode(query)).unwrap();
    assert_eq!(filters["type"], json!(["container", "image", "volume"]));
    let actions: Vec<&str> = filters["event"]
        .as_array()
        .unwrap()
        .iter()
        .map(|action| action.as_str().unwrap())
        .collect();
    for (_, _, watched) in WATCHED {
        for action in watched {
            assert!(actions.contains(action), "{action} is asked for");
        }
    }
    assert!(!actions.iter().any(|action| action.starts_with("exec")));
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            out.push(u8::from_str_radix(&text[index + 1..index + 3], 16).unwrap());
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

#[test]
fn reconnecting_waits_longer_each_time_up_to_half_a_minute() {
    let delays: Vec<u64> = (0..8).map(|n| retry_delay(n).as_secs()).collect();
    assert_eq!(delays, [1, 2, 4, 8, 16, 30, 30, 30]);
    assert_eq!(retry_delay(u32::MAX), RETRY_MAX);
}

/// The next published event, as JSON.
async fn next(events: &mut broadcast::Receiver<Arc<str>>) -> Value {
    let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("an event is published in time")
        .unwrap();
    serde_json::from_str(&event).unwrap()
}

#[tokio::test]
async fn a_burst_is_published_once_with_every_list_it_changed() {
    let app = crate::tests::app(false);
    let mut events = app.events.subscribe();
    let changes = Arc::new(Changes::default());
    let publisher = tokio::spawn(publish(
        app.clone(),
        changes.clone(),
        Duration::from_millis(50),
    ));
    for list in ["containers", "volumes", "containers", "containers"] {
        changes.add(list);
    }
    assert_eq!(
        next(&mut events).await,
        json!({"type": "changed", "lists": ["containers", "volumes"]})
    );
    // Nothing more for that burst; the next change is an event of its own.
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(events.try_recv().is_err(), "one event per burst");
    changes.add("images");
    assert_eq!(
        next(&mut events).await,
        json!({"type": "changed", "lists": ["images"]})
    );
    app.events.close();
    tokio::time::timeout(Duration::from_secs(5), publisher)
        .await
        .expect("the publisher ends with the manager")
        .unwrap();
}

#[cfg(unix)]
mod engine_socket {
    //! The watcher against a fake Engine API that sends events until it is told to restart.
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        sync::mpsc,
    };

    /// Accepts event streams; each sends what arrives on `lines` until `None`, which ends it as a
    /// restarting daemon does.
    async fn engine(listener: tokio::net::UnixListener, mut lines: mpsc::Receiver<Option<String>>) {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let read = socket.read(&mut buffer).await.unwrap();
                request.extend_from_slice(&buffer[..read]);
            }
            let request = String::from_utf8(request).unwrap();
            assert!(
                request.starts_with(&format!("GET {} ", path())),
                "{request}"
            );
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n")
                .await
                .unwrap();
            while let Some(Some(line)) = lines.recv().await {
                let line = format!("{line}\n");
                socket
                    .write_all(format!("{:x}\r\n{line}\r\n", line.len()).as_bytes())
                    .await
                    .unwrap();
            }
            drop(socket);
        }
    }

    #[tokio::test]
    async fn the_watcher_says_when_events_flow_and_follows_docker_through_a_restart() {
        let dir =
            std::env::temp_dir().join(format!("thoric-events-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let socket = dir.join("docker.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let (lines, received) = mpsc::channel(8);
        let server = tokio::spawn(engine(listener, received));

        let mut app = crate::tests::app(false);
        Arc::get_mut(&mut app).unwrap().config.docker_socket = Some(socket);
        let mut events = app.events.subscribe();
        spawn(app.clone());

        assert_eq!(
            next(&mut events).await,
            json!({"type": "docker-events", "live": true})
        );
        assert!(app.events.docker_events());
        lines
            .send(Some(container("exec_start: true", true).to_string()))
            .await
            .unwrap();
        lines
            .send(Some(container("die", true).to_string()))
            .await
            .unwrap();
        assert_eq!(
            next(&mut events).await,
            json!({"type": "changed", "lists": ["containers"]})
        );

        // Docker restarts: the stream ends, is told as lost, and comes back by itself.
        lines.send(None).await.unwrap();
        assert_eq!(
            next(&mut events).await,
            json!({"type": "docker-events", "live": false})
        );
        assert_eq!(
            next(&mut events).await,
            json!({"type": "docker-events", "live": true})
        );

        app.events.close();
        server.abort();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
