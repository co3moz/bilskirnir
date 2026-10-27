//! Connections to desktop MCP servers: a `docker exec` as the desktop user that speaks MCP on its
//! stdio, either the server's own program or the adapter of a Streamable HTTP server on the
//! desktop's loopback. Endpoints keep one connection per server and endpoint, closed once idle.
use super::registry::{ServerRecord, ServerSpec};
use crate::{App, model::Container};
use anyhow::{Context, Result, bail};
use rmcp::{
    ClientHandler, RoleClient, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, Implementation,
        JsonObject, PaginatedRequestParams, ProtocolVersion, ResultType, Tool,
    },
    service::{NotificationContext, Peer, RunningService},
};
use std::{
    collections::HashMap,
    pin::Pin,
    process::Stdio,
    sync::{Arc, Mutex, Weak},
    task::{Context as TaskContext, Poll},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader, ReadBuf},
    process::{Child, ChildStderr, ChildStdin, ChildStdout},
};

/// A server that has not finished its handshake in this time is not going to.
const CONNECT_LIMIT: Duration = Duration::from_secs(120);
/// The longest a tool call may run, such as a test suite.
const CALL_LIMIT: Duration = Duration::from_secs(900);
/// The longest message a server may send; a longer line ends the connection.
const LINE_LIMIT: usize = 16 * 1024 * 1024;
/// How much of a server's stderr is kept.
const LOG_LIMIT: usize = 16 * 1024;
/// A connection unused this long is closed; the next call opens another.
pub(super) const IDLE: Duration = Duration::from_secs(600);
const ADAPTER: &str = include_str!("../../helpers/mcp_http_adapter.py");
/// The desktop user's own programs come first, as in a login shell.
const PATH: &str = "/home/desktop/.local/bin:/home/desktop/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// Hears a server say its tool list changed.
#[derive(Clone)]
pub(super) struct Listener {
    app: Weak<App>,
    container_id: String,
    server: String,
}
impl ClientHandler for Listener {
    fn get_info(&self) -> ClientConfig {
        ClientConfig::new(
            ClientCapabilities::default(),
            Implementation::new("thoric", env!("CARGO_PKG_VERSION")),
        )
        .with_protocol_version(ProtocolVersion::V_2025_11_25)
    }
    async fn on_tool_list_changed(&self, context: NotificationContext<RoleClient>) {
        if let Some(app) = self.app.upgrade() {
            super::watch::reread_tools(app, &self.container_id, &self.server, context.peer);
        }
    }
}

/// One running connection to a server.
pub(super) struct Connection {
    service: RunningService<RoleClient, Listener>,
    /// The `docker exec`, killed when the connection is dropped.
    _child: Child,
    used: Mutex<Instant>,
}
impl Connection {
    fn closed(&self) -> bool {
        self.service.is_transport_closed()
    }
    fn idle(&self) -> Duration {
        self.used.lock().unwrap().elapsed()
    }
    fn touch(&self) {
        *self.used.lock().unwrap() = Instant::now();
    }
}

/// Ends the connection when a server writes a line longer than [`LINE_LIMIT`], before it is held.
pub(super) struct Bounded<R> {
    inner: R,
    run: usize,
}
impl<R> Bounded<R> {
    pub(super) fn new(inner: R) -> Self {
        Self { inner, run: 0 }
    }
}
impl<R: AsyncRead + Unpin> AsyncRead for Bounded<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let this = &mut *self;
        match Pin::new(&mut this.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {}
            other => return other,
        }
        let read = &buf.filled()[before..];
        this.run = match read.iter().rposition(|byte| *byte == b'\n') {
            Some(at) => read.len() - at - 1,
            None => this.run + read.len(),
        };
        if this.run > LINE_LIMIT {
            return Poll::Ready(Err(std::io::Error::other(
                "The server sent a message larger than 16 MiB",
            )));
        }
        Poll::Ready(Ok(()))
    }
}

/// The `docker exec` arguments that start a server, and what its stdin receives first.
pub(super) fn exec_args(container: &str, spec: &ServerSpec) -> Result<Exec> {
    let mut args: Vec<String> = ["exec", "-i", "--user", "1001:1001"]
        .map(str::to_owned)
        .to_vec();
    if let Some(url) = &spec.url {
        args.extend([container, "python3", "-u", "-c", ADAPTER].map(str::to_owned));
        let mut first =
            serde_json::to_vec(&serde_json::json!({"url": url, "headers": spec.headers}))?;
        first.push(b'\n');
        return Ok(Exec {
            args,
            env: Vec::new(),
            first: Some(first),
        });
    }
    let command = spec
        .command
        .as_deref()
        .context("The server has no command")?;
    args.push("--workdir".into());
    args.push(spec.cwd.clone().unwrap_or_else(|| "/home/desktop".into()));
    for variable in [
        "HOME=/home/desktop".to_owned(),
        "USER=desktop".into(),
        "DISPLAY=:0".into(),
        "XDG_RUNTIME_DIR=/run/desktop".into(),
        "PULSE_SERVER=unix:/run/desktop/pulse/native".into(),
        format!("PATH={PATH}"),
    ] {
        args.push("--env".into());
        args.push(variable);
    }
    // A declared variable may hold a resolved compose value: named alone, Docker reads it from
    // the CLI's own environment, so the value is not on a command line `ps` shows.
    let mut env = Vec::new();
    for (key, value) in &spec.env {
        args.push("--env".into());
        args.push(key.clone());
        env.push((key.clone(), value.clone()));
    }
    args.push(container.into());
    args.push(command.into());
    args.extend(spec.args.iter().cloned());
    Ok(Exec {
        args,
        env,
        first: None,
    })
}

/// How a server is started: the `docker exec` arguments, the variables the CLI's environment
/// carries for `--env KEY`, and what the server's stdin receives first.
pub(super) struct Exec {
    pub(super) args: Vec<String>,
    pub(super) env: Vec<(String, String)>,
    pub(super) first: Option<Vec<u8>>,
}

/// A server's `docker exec` and its standard streams, what its stdin receives first written.
pub(super) struct Started {
    pub child: Child,
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
    pub stderr: ChildStderr,
}

/// Start a server of a running container as the desktop user, its `${NAME}` values resolved.
pub(super) async fn start(app: &App, container: &Container, spec: &ServerSpec) -> Result<Started> {
    let spec = spec.resolve(&app.compose_values(&container.id))?;
    let info = app.owned(container).await?;
    if info["State"]["Running"] != true {
        bail!("Start the container before using its MCP servers");
    }
    let docker_id = info["Id"].as_str().context("Missing container ID")?;
    let Exec { args, env, first } = exec_args(docker_id, &spec)?;
    let mut child = app
        .command(&args)
        .envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Could not start Docker CLI")?;
    let mut stdin = child.stdin.take().context("Missing stdin")?;
    let stdout = child.stdout.take().context("Missing stdout")?;
    let stderr = child.stderr.take().context("Missing stderr")?;
    if let Some(first) = first {
        stdin.write_all(&first).await?;
    }
    Ok(Started {
        child,
        stdin,
        stdout,
        stderr,
    })
}

/// Start a server of a running container and finish its MCP handshake.
async fn connect(
    app: &Arc<App>,
    container: &Container,
    record: &ServerRecord,
) -> Result<Connection> {
    let spec = record
        .spec
        .as_ref()
        .with_context(|| format!("The server {} has no valid declaration", record.name))?;
    let Started {
        mut child,
        stdin,
        stdout,
        stderr,
    } = start(app, container, spec).await?;
    let key = ServerRecord::key(&container.id, &record.name);
    let logs = Arc::downgrade(app);
    let log_key = key.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let Some(app) = logs.upgrade() else { return };
            app.desktop_mcp.log(&log_key, &line);
        }
    });
    let listener = Listener {
        app: Arc::downgrade(app),
        container_id: container.id.clone(),
        server: record.name.clone(),
    };
    let transport = (Bounded::new(stdout), stdin);
    let started = tokio::time::timeout(CONNECT_LIMIT, listener.serve(transport)).await;
    let service = match started {
        Ok(Ok(service)) => service,
        failed => {
            // A missing program or a crash at start says why on stderr, which is still arriving.
            let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
            tokio::time::sleep(Duration::from_millis(100)).await;
            let reason = match failed {
                Err(_) => "it did not finish the MCP handshake in 2 minutes".to_owned(),
                Ok(Err(error)) => format!("its MCP handshake failed: {error}"),
                Ok(Ok(_)) => unreachable!(),
            };
            let output = app.desktop_mcp.log_tail(&key, 600);
            if output.is_empty() {
                bail!("The server {} could not be started: {reason}", record.name);
            }
            bail!(
                "The server {} could not be started: {reason}. It wrote: {output}",
                record.name
            );
        }
    };
    Ok(Connection {
        service,
        _child: child,
        used: Mutex::new(Instant::now()),
    })
}

/// Start a server, read its tools and close it again.
pub(super) async fn read_tools(
    app: &Arc<App>,
    container: &Container,
    record: &ServerRecord,
) -> Result<Vec<Tool>> {
    let connection = connect(app, container, record).await?;
    let tools = tokio::time::timeout(CONNECT_LIMIT, list_tools(&connection.service))
        .await
        .context("The server did not list its tools in 2 minutes")?;
    let _ = connection.service.cancel().await;
    tools
}

/// Most tools a desktop server may list; an endpoint carries them all in every `tools/list`.
pub(super) const MAX_TOOLS: usize = 256;
/// Largest tool list, as JSON, a desktop server may give.
pub(super) const MAX_TOOLS_BYTES: usize = 1 << 20;
/// Most pages a tool list is read in; a server that keeps handing out cursors is not read on.
const MAX_TOOL_PAGES: usize = 16;

/// Read a server's tools page by page, within what an endpoint can carry and keep. The server
/// runs inside the desktop, so its answers are bounded here, not trusted.
pub(super) async fn list_tools(peer: &Peer<RoleClient>) -> Result<Vec<Tool>> {
    let mut tools = Vec::new();
    let mut cursor = None;
    for _ in 0..MAX_TOOL_PAGES {
        let page = peer
            .list_tools(Some(PaginatedRequestParams::default().with_cursor(cursor)))
            .await
            .context("The server could not list its tools")?;
        tools.extend(page.tools);
        check_tool_list(&tools)?;
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(tools);
        }
    }
    bail!("The server lists its tools in more than {MAX_TOOL_PAGES} pages")
}

/// Refuse a tool list an endpoint could not carry.
pub(super) fn check_tool_list(tools: &[Tool]) -> Result<()> {
    if tools.len() > MAX_TOOLS {
        bail!("The server lists more than {MAX_TOOLS} tools");
    }
    let bytes = serde_json::to_vec(tools).context("The tool list is not JSON")?;
    if bytes.len() > MAX_TOOLS_BYTES {
        bail!(
            "The server's tool list is larger than {} MiB",
            MAX_TOOLS_BYTES >> 20
        );
    }
    Ok(())
}

type Key = (String, String, String);
type Slot = Arc<tokio::sync::Mutex<Option<Arc<Connection>>>>;

/// Connections by endpoint, container and server. Agents on one endpoint share a server's
/// connection, and so its state, such as a browser's open pages; another endpoint has its own.
#[derive(Default)]
pub(super) struct Pool(Mutex<HashMap<Key, Slot>>);
impl Pool {
    fn slot(&self, key: Key) -> Slot {
        self.0.lock().unwrap().entry(key).or_default().clone()
    }
    /// Keep the connections whose key passes, close the others.
    pub fn retain(&self, keep: impl Fn(&Key) -> bool) {
        self.0.lock().unwrap().retain(|key, _| keep(key));
    }
    /// Close idle and closed connections that no call is opening or using now.
    pub fn reap(&self) {
        self.0
            .lock()
            .unwrap()
            .retain(|_, slot| match slot.try_lock() {
                Ok(held) => held
                    .as_ref()
                    .is_some_and(|c| !c.closed() && c.idle() < IDLE),
                Err(_) => true,
            });
    }
}

/// A desktop server's result as the endpoint passes it on. A server of a protocol revision
/// before 2026-07-28 sends no `resultType`, which a client of that revision requires in every
/// result, so an absent one is the `complete` it means; rmcp drops it again for older clients.
pub(super) fn relayed(mut result: CallToolResult) -> CallToolResult {
    result.result_type.get_or_insert(ResultType::COMPLETE);
    result
}

/// Call a tool of a desktop server for an endpoint, connecting first when needed.
pub(super) async fn call(
    app: &Arc<App>,
    endpoint: &str,
    container: &Container,
    record: &ServerRecord,
    tool: &str,
    arguments: JsonObject,
) -> Result<CallToolResult> {
    let slot = app.desktop_mcp.pool.slot((
        endpoint.to_owned(),
        container.id.clone(),
        record.name.clone(),
    ));
    let connection = {
        let mut held = slot.lock().await;
        match held.as_ref() {
            Some(connection) if !connection.closed() => connection.clone(),
            _ => {
                let connection = Arc::new(connect(app, container, record).await?);
                *held = Some(connection.clone());
                connection
            }
        }
    };
    connection.touch();
    let request = CallToolRequestParams::new(tool.to_owned()).with_arguments(arguments);
    let result = tokio::time::timeout(CALL_LIMIT, connection.service.call_tool(request)).await;
    connection.touch();
    match result {
        Ok(Ok(result)) => Ok(relayed(result)),
        Ok(Err(error)) => {
            if connection.closed() {
                slot.lock().await.take();
            }
            bail!("The server {} failed the call: {error}", record.name)
        }
        Err(_) => bail!(
            "The server {} did not answer in {} minutes",
            record.name,
            CALL_LIMIT.as_secs() / 60
        ),
    }
}

impl super::DesktopMcp {
    pub(super) fn log(&self, key: &str, line: &str) {
        let mut logs = self.logs.lock().unwrap();
        let log = logs.entry(key.to_owned()).or_default();
        log.push_str(line);
        log.push('\n');
        if log.len() > LOG_LIMIT {
            let mut at = log.len() - LOG_LIMIT;
            while !log.is_char_boundary(at) {
                at += 1;
            }
            log.drain(..at);
        }
    }
    /// The last `limit` bytes of a server's stderr.
    pub(super) fn log_tail(&self, key: &str, limit: usize) -> String {
        let logs = self.logs.lock().unwrap();
        let Some(log) = logs.get(key) else {
            return String::new();
        };
        let mut at = log.len().saturating_sub(limit);
        while !log.is_char_boundary(at) {
            at += 1;
        }
        log[at..].trim().to_owned()
    }
}
