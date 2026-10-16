//! The host's GPU, sampled with `nvidia-smi` once a second while a GPU desktop runs.
//!
//! WSL gives Linux the GPU as a paravirtual device. NVML reports the whole adapter there, Windows
//! applications included, and neither per-process nor per-container figures, so this is the
//! host's utilization and memory, not a desktop's share. Other adapters have no such tool in
//! WSL and are reported as unavailable.
//!
//! The manager container has no GPU device, so in Docker it samples through a helper container
//! from its own image with the same device a GPU desktop gets, no network, a read-only root, no
//! capabilities and the `nobody` user. The helper's stdin is attached: Docker closes it when the
//! manager stops listening, and the helper's shell then ends `nvidia-smi` and the container goes.
//! A manager run outside Docker, as in development, runs `nvidia-smi` itself.
use crate::{App, config::Config};
use anyhow::{Result, bail};
use std::{process::Stdio, sync::Arc, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

/// The driver's `nvidia-smi` in a WSL distribution.
const WSL_SMI: &str = "/usr/lib/wsl/lib/nvidia-smi";
const QUERY: [&str; 4] = [
    "--query-gpu=index,name,utilization.gpu,memory.used,memory.total",
    "--format=csv,noheader,nounits",
    "-l",
    "1",
];
/// `nvidia-smi` prints one short line per GPU; a longer one is not one.
const LINE_LIMIT: u64 = 1024;
/// Docker may take a while to start the helper; after that a line arrives every second.
const FIRST_LINE: Duration = Duration::from_secs(60);
const NEXT_LINE: Duration = Duration::from_secs(15);

/// One GPU's line. A field the driver does not report, such as `[N/A]`, is `None`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GpuReading {
    pub index: u32,
    pub name: String,
    pub utilization: Option<f32>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
}

/// A line of `index, name, utilization %, used MiB, total MiB`. The name may hold commas, so the
/// fields are counted from both ends.
pub(crate) fn parse(line: &str) -> Option<GpuReading> {
    let fields: Vec<&str> = line.trim().split(", ").collect();
    if fields.len() < 5 {
        return None;
    }
    let (numbers, name) = (&fields[fields.len() - 3..], &fields[1..fields.len() - 3]);
    let mib = |text: &str| text.parse::<u64>().ok().map(|mib| mib * 1024 * 1024);
    Some(GpuReading {
        index: fields[0].parse().ok()?,
        name: name.join(", "),
        utilization: numbers[0]
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite()),
        memory_used: mib(numbers[1]),
        memory_total: mib(numbers[2]),
    })
}

/// How the GPU is reached: the WSL device, or the NVIDIA Container Toolkit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Route {
    Wsl,
    Toolkit,
}

/// The routes this installation gives desktops, in the order they are tried.
pub(crate) fn routes(config: &Config) -> Vec<Route> {
    let mut routes = Vec::new();
    if config.wsl_gpu {
        routes.push(Route::Wsl);
    }
    if config.cuda {
        routes.push(Route::Toolkit);
    }
    routes
}

fn program(route: Route) -> &'static str {
    match route {
        Route::Wsl => WSL_SMI,
        // The toolkit puts its own nvidia-smi on the PATH.
        Route::Toolkit => "nvidia-smi",
    }
}

/// The arguments that run `nvidia-smi` in a helper from `image` until its stdin closes.
pub(crate) fn helper_args(route: Route, image: &str, name: &str) -> Vec<String> {
    let mut args: Vec<String> = [
        "run",
        "--rm",
        "--interactive",
        "--name",
        name,
        "--label",
        "io.wsl-desktop.usage-helper=true",
        "--network",
        "none",
        "--read-only",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges",
        "--user",
        "65534:65534",
    ]
    .map(str::to_owned)
    .into();
    match route {
        Route::Wsl => args.extend(
            [
                "--device",
                "/dev/dxg",
                "--mount",
                "type=bind,src=/usr/lib/wsl,dst=/usr/lib/wsl,readonly",
                "--env",
                "LD_LIBRARY_PATH=/usr/lib/wsl/lib",
            ]
            .map(str::to_owned),
        ),
        Route::Toolkit => args.extend(["--gpus".into(), "all".into()]),
    }
    let query = QUERY.join(" ");
    args.extend([
        "--entrypoint".into(),
        "sh".into(),
        image.into(),
        "-c".into(),
        format!("{} {query} & cat >/dev/null; kill $!", program(route)),
    ]);
    args
}

impl App {
    /// The command that samples the GPU on this route: a helper next to a manager in Docker, or
    /// `nvidia-smi` itself.
    async fn gpu_command(&self, route: Route) -> Result<tokio::process::Command> {
        let Some(manager) = &self.config.manager else {
            let mut command = tokio::process::Command::new(program(route));
            command.args(QUERY);
            if route == Route::Wsl {
                command.env("LD_LIBRARY_PATH", "/usr/lib/wsl/lib");
            }
            return Ok(command);
        };
        let info = self.inspect("container", manager).await?;
        if info["Config"]["Labels"]["io.wsl-desktop.manager"] != "true" {
            bail!("MANAGER_CONTAINER does not point to the manager");
        }
        let image = info["Image"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing manager image"))?;
        let name = format!(
            "wsld-usage-gpu-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..12]
        );
        Ok(self.command(&helper_args(route, image, &name)))
    }
}

/// Sample until the sampler is stopped, trying each route in turn; returns why none works.
pub(crate) async fn sample(app: Arc<App>) -> String {
    let mut reason = String::from("The installation gives desktops no GPU");
    for route in routes(&app.config) {
        let result = async {
            let command = app.gpu_command(route).await?;
            follow(&app, command).await
        }
        .await;
        if let Err(error) = result {
            reason = format!("{error:#}");
            tracing::debug!("GPU usage through {route:?}: {reason}");
        }
    }
    reason
}

/// Record each line the command prints. It ends only with an error, since `nvidia-smi -l` never
/// stops on its own.
async fn follow(app: &App, mut command: tokio::process::Command) -> Result<()> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    // Held open for as long as the sampler runs; closing it ends a helper.
    let _stdin = child.stdin.take();
    let mut stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
    let mut stderr = child.stderr.take().expect("piped stderr");
    let errors = tokio::spawn(async move {
        let mut text = Vec::new();
        let _ = (&mut stderr).take(4096).read_to_end(&mut text).await;
        // Whatever follows is drained, so a chatty helper never blocks on a full pipe.
        let _ = tokio::io::copy(&mut stderr, &mut tokio::io::sink()).await;
        String::from_utf8_lossy(&text).trim().to_owned()
    });
    let mut first: Option<String> = None;
    let mut seen = false;
    let mut line = Vec::new();
    let ended = loop {
        line.clear();
        let wait = if seen { NEXT_LINE } else { FIRST_LINE };
        let mut limited = (&mut stdout).take(LINE_LIMIT);
        match tokio::time::timeout(wait, limited.read_until(b'\n', &mut line)).await {
            Err(_) => break "nvidia-smi stopped reporting".to_owned(),
            Ok(Err(error)) => return Err(error.into()),
            Ok(Ok(0)) => break String::new(),
            Ok(Ok(_)) => {
                let text = String::from_utf8_lossy(&line);
                match parse(&text) {
                    Some(reading) => {
                        seen = true;
                        app.usage.record_gpu(&reading);
                    }
                    // nvidia-smi explains a missing driver library on stdout.
                    None if first.is_none() && !text.trim().is_empty() => {
                        first = Some(text.trim().to_owned());
                    }
                    None => {}
                }
            }
        }
    };
    let _ = child.start_kill();
    let _ = child.wait().await;
    let stderr = errors.await.unwrap_or_default();
    let reason = [first.unwrap_or_default(), stderr, ended]
        .into_iter()
        .find(|text| !text.is_empty())
        .unwrap_or_else(|| "nvidia-smi ended".into());
    bail!(reason)
}

#[cfg(test)]
#[path = "gpu_tests.rs"]
mod tests;
