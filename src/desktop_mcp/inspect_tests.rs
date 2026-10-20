//! The inspector's relay: which servers a first frame may name and what refuses to open, that
//! the browser's frames reach the server one message a line and nothing else does, how the
//! server's output reaches the browser, that the browser hears why a server stopped, and that
//! `/mcp` and an endpoint answer through the socket, from the dashboard's origin only.
use super::*;
use crate::{
    desktop_mcp::{
        registry::Source,
        scopes::Scope,
        tests::{container, endpoint, grant, names, server},
    },
    model::CONTAINERS,
    tests::{app, mcp_app},
};
use std::{
    convert::Infallible,
    net::SocketAddr,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, DuplexStream},
    net::TcpStream,
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
};

const WAIT: Duration = Duration::from_secs(10);

/// The browser's end of a relay: what it sends, and what the relay sent it.
struct Browser {
    frames: Option<UnboundedSender<Message>>,
    sent: UnboundedReceiver<Message>,
}
impl Browser {
    fn send(&self, text: &str) {
        self.frames
            .as_ref()
            .unwrap()
            .send(Message::Text(text.into()))
            .unwrap();
    }
    fn send_message(&self, message: Message) {
        self.frames.as_ref().unwrap().send(message).unwrap();
    }
    /// The next text frame the relay sent, as it was sent.
    async fn text(&mut self) -> String {
        loop {
            match tokio::time::timeout(WAIT, self.sent.recv()).await {
                Ok(Some(Message::Text(text))) => return text.to_string(),
                Ok(Some(Message::Ping(_))) => continue,
                other => panic!("expected a text frame, got {other:?}"),
            }
        }
    }
    async fn frame(&mut self) -> Value {
        serde_json::from_str(&self.text().await).unwrap()
    }
    /// Whether the relay closed the socket without sending anything more.
    async fn closed(&mut self) -> bool {
        loop {
            match tokio::time::timeout(WAIT, self.sent.recv()).await {
                Ok(Some(Message::Ping(_))) => continue,
                Ok(None) => return true,
                _ => return false,
            }
        }
    }
}

/// Run a relay to `peer` on a socket double.
fn relayed(peer: Peer) -> Browser {
    let (to_browser, sent) = unbounded_channel::<Message>();
    let sink = Box::pin(futures_util::sink::unfold(
        to_browser,
        |to_browser, message: Message| async move {
            to_browser.send(message).map_err(|_| ())?;
            Ok::<_, ()>(to_browser)
        },
    ));
    let (frames, from_browser) = unbounded_channel::<Message>();
    let stream = Box::pin(futures_util::stream::unfold(
        from_browser,
        |mut from_browser| async move {
            from_browser
                .recv()
                .await
                .map(|message| (Ok::<_, Infallible>(message), from_browser))
        },
    ));
    tokio::spawn(relay(sink, stream, peer));
    Browser {
        frames: Some(frames),
        sent,
    }
}

/// The server's end of a relay: what it reads, and where it writes.
struct Server {
    input: BufReader<DuplexStream>,
    output: Option<DuplexStream>,
    errors: Option<DuplexStream>,
}
impl Server {
    async fn line(&mut self) -> String {
        let mut line = String::new();
        tokio::time::timeout(WAIT, self.input.read_line(&mut line))
            .await
            .unwrap()
            .unwrap();
        line
    }
    async fn write(&mut self, text: &str) {
        self.output
            .as_mut()
            .unwrap()
            .write_all(text.as_bytes())
            .await
            .unwrap();
    }
    async fn write_error(&mut self, text: &str) {
        self.errors
            .as_mut()
            .unwrap()
            .write_all(text.as_bytes())
            .await
            .unwrap();
    }
}

/// A server the test plays, which says `reason` when its output ends.
fn played(reason: &str) -> (Peer, Server) {
    let (input, server_input) = tokio::io::duplex(PIPE);
    let (server_output, output) = tokio::io::duplex(PIPE);
    let (server_errors, errors) = tokio::io::duplex(PIPE);
    let reason = reason.to_owned();
    let stop = CancellationToken::new();
    let peer = Peer {
        output: Box::new(output),
        input: Box::new(input),
        errors: Some(Box::new(errors)),
        end: End::Service {
            task: tokio::spawn(async move { Err(reason) }),
            _stop: stop.drop_guard(),
        },
    };
    let server = Server {
        input: BufReader::new(server_input),
        output: Some(server_output),
        errors: Some(server_errors),
    };
    (peer, server)
}

#[tokio::test]
async fn a_message_reaches_the_server_on_a_line_and_its_answer_comes_back_as_written() {
    let (peer, mut server) = played("stopped");
    let mut browser = relayed(peer);
    let ping = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
    browser.send(ping);
    assert_eq!(server.line().await, format!("{ping}\n"));

    // Key order, spacing and numbers stay as the server wrote them.
    let answer = r#"{"jsonrpc":"2.0", "id":1, "result":{"z":1,"a":1.50}}"#;
    server.write(&format!("{answer}\r\n")).await;
    assert_eq!(
        browser.text().await,
        format!(r#"{{"type":"message","message":{answer}}}"#)
    );

    // A blank line is nothing; a line that is not JSON is the server's stdout.
    server.write("\n   \nListening on stdio\n").await;
    assert_eq!(
        browser.frame().await,
        json!({"type": "stdout", "line": "Listening on stdio", "truncated": false})
    );
    server.write_error("warning: no config\n").await;
    assert_eq!(
        browser.frame().await,
        json!({"type": "stderr", "line": "warning: no config", "truncated": false})
    );
}

#[tokio::test]
async fn a_frame_that_is_not_one_message_on_one_line_is_refused_and_not_passed_on() {
    let (peer, mut server) = played("stopped");
    let mut browser = relayed(peer);
    for refused in [
        "{\"jsonrpc\":\"2.0\",\"method\":\"a\"}\n{\"jsonrpc\":\"2.0\",\"method\":\"b\"}",
        "{\"jsonrpc\":\"2.0\",\r\"method\":\"a\"}",
        "not json",
        "5",
        "\"ping\"",
        "{\"jsonrpc\":\"2.0\"} trailing",
    ] {
        browser.send(refused);
        let frame = browser.frame().await;
        assert_eq!(frame["type"], "error", "{refused:?}: {frame}");
    }
    browser.send_message(Message::Binary(vec![b'{', b'}'].into()));
    assert_eq!(browser.frame().await["type"], "error");

    let batch = r#"[{"jsonrpc":"2.0","id":2,"method":"ping"}]"#;
    browser.send(batch);
    assert_eq!(
        server.line().await,
        format!("{batch}\n"),
        "only the batch got through"
    );
}

#[tokio::test]
async fn the_browser_hears_why_the_server_stopped_after_what_it_wrote_last() {
    let (peer, mut server) = played("The MCP handshake failed: expected initialize");
    let mut browser = relayed(peer);
    server.write_error("panicked at main.rs\n").await;
    server.errors.take();
    server.output.take();
    assert_eq!(
        browser.frame().await,
        json!({"type": "stderr", "line": "panicked at main.rs", "truncated": false})
    );
    assert_eq!(
        browser.frame().await,
        json!({"type": "closed", "reason": "The MCP handshake failed: expected initialize"})
    );
    assert!(browser.closed().await, "the socket closes after the reason");
}

#[tokio::test]
async fn when_the_browser_leaves_the_server_reads_the_end_of_its_input() {
    let (peer, mut server) = played("stopped");
    let mut browser = relayed(peer);
    browser.frames.take();
    let mut rest = Vec::new();
    tokio::time::timeout(WAIT, server.input.read_to_end(&mut rest))
        .await
        .unwrap()
        .unwrap();
    assert!(rest.is_empty());
    assert!(
        browser.closed().await,
        "and nothing tells a browser that left"
    );
}

#[tokio::test]
async fn a_line_longer_than_a_message_may_be_ends_the_server() {
    let (peer, mut server) = played("stopped");
    let mut browser = relayed(peer);
    let mut output = server.output.take().unwrap();
    tokio::spawn(async move {
        let block = vec![b'a'; 1 << 20];
        for _ in 0..17 {
            if output.write_all(&block).await.is_err() {
                return;
            }
        }
    });
    let frame = browser.frame().await;
    assert_eq!(frame["type"], "error", "{frame}");
    assert!(
        frame["message"].as_str().unwrap().contains("16 MiB"),
        "{frame}"
    );
    assert_eq!(browser.frame().await["type"], "closed");
    // Its stderr is still open, and does not keep the socket open.
    assert!(server.errors.is_some());
    assert!(browser.closed().await);
}

#[test]
fn output_that_is_not_a_message_is_cut_where_a_character_ends() {
    let long = "é".repeat(LINE_SHOWN);
    let frame: Value = serde_json::from_str(&line_frame("stderr", long.as_bytes())).unwrap();
    let line = frame["line"].as_str().unwrap();
    assert!(line.len() <= LINE_SHOWN && line.len() > LINE_SHOWN - 2);
    assert!(line.chars().all(|c| c == 'é'));
    assert_eq!(frame["truncated"], true);

    // Invalid UTF-8 is shown, not dropped.
    let frame: Value = serde_json::from_str(&output_frame(b"\xff\xfe ready\n").unwrap()).unwrap();
    assert_eq!(frame["type"], "stdout");
    assert!(frame["line"].as_str().unwrap().ends_with(" ready"));
    assert_eq!(output_frame(b" \r\n"), None);
}

#[test]
fn a_first_frame_names_one_kind_of_server() {
    assert!(matches!(
        target(r#"{"kind":"server","container":"c1","name":"chrome"}"#),
        Ok(Target::Server { container, name }) if container == "c1" && name == "chrome"
    ));
    assert!(matches!(
        target(r#"{"kind":"trial","container":"c1","server":{"name":"try","command":"npx"}}"#),
        Ok(Target::Trial { server, .. }) if server.name == "try" && server.spec.command.as_deref() == Some("npx")
    ));
    assert!(matches!(
        target(r#"{"kind":"endpoint","name":"agent"}"#),
        Ok(Target::Endpoint { name }) if name == "agent"
    ));
    assert!(matches!(
        target(r#"{"kind":"administrator"}"#),
        Ok(Target::Administrator)
    ));
    for refused in [
        r#"{"kind":"host","command":"bash"}"#,
        r#"{"kind":"server","container":"c1"}"#,
        r#"{"kind":"trial","container":"c1","server":{"name":"try","command":"npx","root":true}}"#,
        r#"{"container":"c1","name":"chrome"}"#,
        "[]",
    ] {
        assert!(target(refused).is_err(), "{refused}");
    }
}

/// Why `target` does not open.
async fn refusal(app: &Arc<App>, target: Target) -> (StatusCode, String) {
    match open(app, target).await {
        Ok(_) => panic!("the target opened"),
        Err(Error(status, message)) => (status, message),
    }
}

#[tokio::test]
async fn a_server_opens_only_when_it_is_known_and_declared_and_mcp_targets_only_while_mcp_is_on() {
    let app = app(false);
    app.db
        .put(CONTAINERS, "c1", &container("c1", "Chrome dev"))
        .unwrap();
    let (status, _) = refusal(
        &app,
        Target::Server {
            container: "gone".into(),
            name: "chrome".into(),
        },
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, message) = refusal(
        &app,
        Target::Server {
            container: "c1".into(),
            name: "chrome".into(),
        },
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(message.contains("chrome"), "{message}");

    let broken = ServerRecord {
        container_id: "c1".into(),
        name: "broken".into(),
        source: Source::Container,
        spec: None,
        tools: Vec::new(),
        tools_at: None,
        error: Some("broken.json: expected value".into()),
    };
    app.db
        .put(SERVERS, &ServerRecord::key("c1", "broken"), &broken)
        .unwrap();
    let (status, message) = refusal(
        &app,
        Target::Server {
            container: "c1".into(),
            name: "broken".into(),
        },
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(message.contains("expected value"), "{message}");

    // A trial is checked as a registration would be, before anything starts.
    let Ok(Target::Trial { server, .. }) = target(
        r#"{"kind":"trial","container":"c1","server":{"name":"try","url":"http://example.com:80/mcp"}}"#,
    ) else {
        panic!("a trial");
    };
    let (status, message) = refusal(
        &app,
        Target::Trial {
            container: "c1".into(),
            server,
        },
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(message.contains("127.0.0.1"), "{message}");

    for target in [
        Target::Administrator,
        Target::Endpoint { name: "c1".into() },
    ] {
        let (status, message) = refusal(&app, target).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(message.contains("MCP_ENABLED"), "{message}");
    }
    let (status, message) = refusal(
        &mcp_app(),
        Target::Endpoint {
            name: "nobody".into(),
        },
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(message.contains("nobody"), "{message}");
}

const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"inspector-test","version":"1"}}}"#;
const INITIALIZED: &str = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
const LIST: &str = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;

/// The tool names of a `tools/list` answer.
fn tool_names(frame: &Value) -> Vec<String> {
    assert_eq!(frame["type"], "message", "{frame}");
    frame["message"]["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("{frame}"))
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn the_administrators_mcp_answers_through_the_inspector_without_a_token() {
    let app = mcp_app();
    let (server, peer) = open(&app, Target::Administrator)
        .await
        .map_err(|e| e.1)
        .unwrap();
    assert_eq!(server, json!({"kind": "administrator", "path": "/mcp"}));
    let mut browser = relayed(peer);
    browser.send(INITIALIZE);
    let frame = browser.frame().await;
    assert_eq!(
        frame["message"]["result"]["serverInfo"]["name"], "bilskirnir",
        "{frame}"
    );
    browser.send(INITIALIZED);
    browser.send(LIST);
    let tools = tool_names(&browser.frame().await);
    assert!(tools.iter().any(|name| name == "get_settings"), "{tools:?}");
    browser.send(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_settings","arguments":{}}}"#);
    let frame = browser.frame().await;
    assert_eq!(
        frame["message"]["result"]["structuredContent"]["upload_limit_bytes"],
        1024 * 1024,
        "{frame}"
    );
}

#[tokio::test]
async fn an_endpoint_offers_the_inspector_the_tools_an_agent_sees() {
    let app = mcp_app();
    app.db
        .put(CONTAINERS, "c1", &container("c1", "Chrome dev"))
        .unwrap();
    server(&app, "c1", "chrome", &["browser_navigate", "browser_click"]);
    let mut agent = endpoint("agent", vec![grant("c1", "chrome-dev", &[Scope::View])]);
    agent.grants[0].excluded_tools = vec!["chrome__browser_click".into()];
    app.db.put(ENDPOINTS, "agent", &agent).unwrap();
    let (server, peer) = open(
        &app,
        Target::Endpoint {
            name: "agent".into(),
        },
    )
    .await
    .map_err(|e| e.1)
    .unwrap();
    assert_eq!(server["path"], "/mcp/agent");
    let mut browser = relayed(peer);
    browser.send(INITIALIZE);
    let frame = browser.frame().await;
    assert_eq!(
        frame["message"]["result"]["serverInfo"]["name"], "bilskirnir-agent",
        "{frame}"
    );
    browser.send(INITIALIZED);
    browser.send(LIST);
    let tools = tool_names(&browser.frame().await);
    assert_eq!(tools, names(&app, &agent));
    assert!(tools.iter().any(|name| name == "chrome__browser_navigate"));
    assert!(!tools.iter().any(|name| name == "chrome__browser_click"));
}

/// A WebSocket client of just what the tests need: masked text frames out, whole frames in.
struct Client(TcpStream);
impl Client {
    /// Open `/api/mcp/inspect` with `headers`; the status line and the client once it switched.
    async fn connect(address: SocketAddr, headers: &str) -> (String, Self) {
        let mut socket = TcpStream::connect(address).await.unwrap();
        socket
            .write_all(
                format!(
                    "GET /api/mcp/inspect HTTP/1.1\r\nHost: localhost:8080\r\n{headers}Connection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            head.push(socket.read_u8().await.unwrap());
        }
        let head = String::from_utf8(head).unwrap();
        (head.lines().next().unwrap().to_owned(), Self(socket))
    }
    async fn send(&mut self, text: &str) {
        let payload = text.as_bytes();
        let mut frame = vec![0x81];
        match payload.len() {
            length @ 0..126 => frame.push(0x80 | length as u8),
            length @ 126..=0xffff => {
                frame.push(0x80 | 126);
                frame.extend((length as u16).to_be_bytes());
            }
            length => {
                frame.push(0x80 | 127);
                frame.extend((length as u64).to_be_bytes());
            }
        }
        let mask = [7u8, 1, 9, 3];
        frame.extend(mask);
        frame.extend(payload.iter().zip(mask.iter().cycle()).map(|(b, m)| b ^ m));
        self.0.write_all(&frame).await.unwrap();
    }
    /// The next text frame, skipping pings.
    async fn receive(&mut self) -> Value {
        loop {
            let (opcode, payload) = tokio::time::timeout(WAIT, self.frame())
                .await
                .expect("a frame");
            match opcode {
                1 => return serde_json::from_slice(&payload).unwrap(),
                9 => continue,
                other => panic!("frame {other}: {}", String::from_utf8_lossy(&payload)),
            }
        }
    }
    async fn frame(&mut self) -> (u8, Vec<u8>) {
        let first = self.0.read_u8().await.unwrap();
        let second = self.0.read_u8().await.unwrap();
        let length = match second & 0x7f {
            126 => u64::from(self.0.read_u16().await.unwrap()),
            127 => self.0.read_u64().await.unwrap(),
            length => u64::from(length),
        };
        let mut payload = vec![0; usize::try_from(length).unwrap()];
        self.0.read_exact(&mut payload).await.unwrap();
        (first & 0x0f, payload)
    }
}

#[tokio::test]
async fn the_inspector_opens_from_a_signed_in_dashboard_on_its_own_origin() {
    let app = mcp_app();
    app.sessions.lock().unwrap().insert(
        "signed-in".into(),
        Instant::now() + Duration::from_secs(600),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = crate::router(app.clone());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let session = "Cookie: bilskirnir_session=signed-in\r\n";

    for (headers, status) in [
        (String::new(), "401"),
        ("Origin: http://localhost:8080\r\n".to_owned(), "401"),
        (session.to_owned(), "403"),
        (format!("{session}Origin: http://evil.example\r\n"), "403"),
        (format!("{session}Origin: http://localhost:9090\r\n"), "403"),
    ] {
        let (line, _) = Client::connect(address, &headers).await;
        assert!(line.contains(status), "{headers:?}: {line}");
    }

    let (line, mut client) = Client::connect(
        address,
        &format!("{session}Origin: http://localhost:8080\r\n"),
    )
    .await;
    assert!(line.starts_with("HTTP/1.1 101"), "{line}");
    client.send(r#"{"kind":"administrator"}"#).await;
    assert_eq!(client.receive().await["type"], "ready");
    client.send(INITIALIZE).await;
    let frame = client.receive().await;
    assert_eq!(
        frame["message"]["result"]["serverInfo"]["name"], "bilskirnir",
        "{frame}"
    );
    client.send(INITIALIZED).await;
    client.send(LIST).await;
    assert!(
        tool_names(&client.receive().await)
            .iter()
            .any(|name| name == "list_mcp_servers")
    );

    let (_, mut second) = Client::connect(
        address,
        &format!("{session}Origin: http://localhost:8080\r\n"),
    )
    .await;
    second.send(r#"{"kind":"desktop"}"#).await;
    let frame = second.receive().await;
    assert_eq!(frame["type"], "error", "{frame}");
    assert_eq!(second.frame().await.0, 8, "and the socket closes");
}
