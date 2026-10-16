//! Pins down how the host GPU is sampled: `nvidia-smi` lines as WSL's driver prints them, fields
//! the driver leaves out, and a helper container that gets the GPU device and nothing else.
use super::*;

#[test]
fn lines_are_read_as_wsl_prints_them() {
    assert_eq!(
        parse("0, NVIDIA GeForce RTX 4080, 13, 3019, 16376\n"),
        Some(GpuReading {
            index: 0,
            name: "NVIDIA GeForce RTX 4080".into(),
            utilization: Some(13.0),
            memory_used: Some(3019 << 20),
            memory_total: Some(16376 << 20),
        })
    );
    // A field the driver does not report stays unknown instead of reading as zero.
    let partial = parse("1, Tesla T4, [N/A], [Not Supported], 15360").unwrap();
    assert_eq!(
        (
            partial.utilization,
            partial.memory_used,
            partial.memory_total
        ),
        (None, None, Some(15360 << 20))
    );
    // A name with a comma keeps it.
    assert_eq!(
        parse("2, Some GPU, Rev 2, 50, 1, 2").unwrap().name,
        "Some GPU, Rev 2"
    );
}

#[test]
fn a_driver_message_is_not_a_reading() {
    for line in [
        "NVIDIA-SMI couldn't find libnvidia-ml.so library in your system.",
        "No devices were found",
        "",
        "x, NVIDIA, 1, 2, 3",
    ] {
        assert_eq!(parse(line), None, "{line}");
    }
}

#[test]
fn routes_follow_what_the_installation_gives_desktops() {
    let mut config = crate::tests::app(false);
    let config = &mut std::sync::Arc::get_mut(&mut config).unwrap().config;
    assert_eq!(routes(config), [Route::Wsl, Route::Toolkit]);
    config.wsl_gpu = false;
    assert_eq!(routes(config), [Route::Toolkit]);
    config.cuda = false;
    assert!(routes(config).is_empty());
}

#[test]
fn the_helper_gets_the_gpu_and_nothing_else() {
    let args = helper_args(Route::Wsl, "sha256:manager", "wsld-usage-gpu-1");
    let text = args.join(" ");
    for expected in [
        "run --rm --interactive --name wsld-usage-gpu-1",
        "--network none --read-only --cap-drop ALL --security-opt no-new-privileges --user 65534:65534",
        "--device /dev/dxg --mount type=bind,src=/usr/lib/wsl,dst=/usr/lib/wsl,readonly",
        "--entrypoint sh sha256:manager -c",
    ] {
        assert!(text.contains(expected), "{expected} in {text}");
    }
    assert!(!text.contains("docker.sock"), "{text}");
    // Stdin closes when the manager stops listening, and the shell then ends nvidia-smi.
    assert_eq!(
        args.last().unwrap(),
        "/usr/lib/wsl/lib/nvidia-smi --query-gpu=index,name,utilization.gpu,memory.used,memory.total --format=csv,noheader,nounits -l 1 & cat >/dev/null; kill $!"
    );
    let toolkit = helper_args(Route::Toolkit, "sha256:manager", "wsld-usage-gpu-2").join(" ");
    assert!(toolkit.contains("--gpus all --entrypoint sh"), "{toolkit}");
    assert!(!toolkit.contains("/dev/dxg"), "{toolkit}");
    assert!(toolkit.ends_with("-c nvidia-smi --query-gpu=index,name,utilization.gpu,memory.used,memory.total --format=csv,noheader,nounits -l 1 & cat >/dev/null; kill $!"), "{toolkit}");
}

#[tokio::test]
async fn a_failing_sampler_reports_what_it_printed() {
    let app = crate::tests::app(false);
    // nvidia-smi explains a missing library on stdout and exits.
    let mut command = tokio::process::Command::new(if cfg!(windows) { "cmd" } else { "sh" });
    if cfg!(windows) {
        command.args([
            "/C",
            "echo NVIDIA-SMI couldn't find libnvidia-ml.so library",
        ]);
    } else {
        command.args([
            "-c",
            "echo \"NVIDIA-SMI couldn't find libnvidia-ml.so library\"",
        ]);
    }
    let error = follow(&app, command).await.unwrap_err();
    assert_eq!(
        format!("{error:#}"),
        "NVIDIA-SMI couldn't find libnvidia-ml.so library"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_sampler_records_each_line_until_it_ends() {
    let app = crate::tests::app(false);
    let mut command = tokio::process::Command::new("sh");
    command.args([
        "-c",
        "echo '0, NVIDIA GeForce RTX 4080, 13, 3019, 16376'; echo '0, NVIDIA GeForce RTX 4080, 20, 3020, 16376'; echo 'gone' >&2",
    ]);
    let error = follow(&app, command).await.unwrap_err();
    assert_eq!(format!("{error:#}"), "gone");
    let store = app.usage.store();
    assert_eq!(store.gpus[&0].samples.len(), 2);
    assert_eq!(store.gpu_state, crate::usage::history::GpuState::Sampling);
}
