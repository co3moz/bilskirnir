//! Pins down what the usage history keeps: at most 30 minutes per series, stamps that only grow
//! so `since` never skips a sample of another container, and a deleted container's history kept
//! under its last name until it ages out.
use super::*;

fn measure(cpu: f32) -> Measure {
    Measure {
        cpu,
        memory: 512 << 20,
        memory_limit: 8 << 30,
        online_cpus: 16,
        disk_read: 0.0,
        disk_write: 4096.0,
        net_rx: 1500.4,
        net_tx: 0.6,
    }
}

#[test]
fn a_full_ring_forgets_its_oldest_sample() {
    let mut store = Store::default();
    for second in 0..CAPACITY as u64 + 5 {
        store.record("a", 1_000_000 + second * 1000, &measure(second as f32));
    }
    let series = &store.containers["a"];
    assert_eq!(series.samples.len(), CAPACITY);
    let first = series.samples.since(0).next().unwrap();
    assert_eq!(first.cpu, 5.0, "the five oldest samples went");
    // Reserved once; a full ring never grows.
    assert!(series.samples.samples.capacity() >= CAPACITY);
    assert!(series.samples.samples.capacity() < 2 * CAPACITY);
}

#[test]
fn since_returns_what_came_after_a_cursor_across_containers() {
    let mut store = Store::default();
    store.record("a", 1000, &measure(1.0));
    store.record("b", 1000, &measure(2.0));
    let cursor = store.cursor();
    // Two streams reporting in the same millisecond still get distinct, ordered stamps.
    store.record("b", 1000, &measure(3.0));
    store.record("a", 999, &measure(4.0));
    let after = |id: &str| -> Vec<f32> {
        store.containers[id]
            .samples
            .since(cursor)
            .map(|s| s.cpu)
            .collect()
    };
    assert_eq!(after("a"), [4.0]);
    assert_eq!(after("b"), [3.0]);
    assert_eq!(store.cursor(), 1003);
    assert_eq!(store.containers["a"].samples.since(0).count(), 2);
}

#[test]
fn stamps_keep_growing_when_the_wall_clock_steps_back() {
    let mut store = Store::default();
    store.record("a", 5000, &measure(1.0));
    store.record("a", 1000, &measure(2.0));
    let stamps: Vec<u64> = store.containers["a"]
        .samples
        .since(0)
        .map(|s| s.t)
        .collect();
    assert_eq!(stamps, [5000, 5001]);
}

fn label(name: &str) -> Option<Label> {
    Some(Label {
        name: name.into(),
        cpu_limit: Some(2.0),
    })
}

#[test]
fn pruning_keeps_deleted_containers_until_their_samples_age_out() {
    let mut store = Store::default();
    let start = 10 * WINDOW_MS;
    store.record_gpu(
        start,
        &GpuReading {
            index: 0,
            name: "GPU".into(),
            utilization: Some(5.0),
            memory_used: Some(1),
            memory_total: Some(2),
        },
    );
    store.record("stopped", start, &measure(1.0));
    store.record("running", start, &measure(1.0));
    store.record("deleted", start + 1000, &measure(1.0));
    store.record("foreign", start + 1000, &measure(1.0));
    // Every container but one another manager owns has a record.
    store.prune(start + 1000, |id| label(id).filter(|_| id != "foreign"));
    assert!(
        !store.containers.contains_key("foreign"),
        "never had a record"
    );
    assert_eq!(store.containers["deleted"].label, label("deleted"));
    assert_eq!(store.gpus.len(), 1);

    // Deleted, it keeps its samples and the name it last had; a renamed one takes its new name.
    store.prune(start + 2000, |id| match id {
        "deleted" => None,
        "running" => label("renamed"),
        _ => label(id),
    });
    assert_eq!(store.containers["deleted"].samples.len(), 1);
    assert_eq!(store.containers["deleted"].label, label("deleted"));
    assert_eq!(store.containers["running"].label, label("renamed"));

    // Once their samples age out, stopped and deleted series go.
    store.record("running", start + WINDOW_MS, &measure(2.0));
    store.prune(start + WINDOW_MS + 1010, |id| {
        label(id).filter(|_| id != "deleted")
    });
    assert!(!store.containers.contains_key("stopped"));
    assert!(!store.containers.contains_key("deleted"));
    assert!(store.gpus.is_empty());
    let running: Vec<f32> = store.containers["running"]
        .samples
        .since(0)
        .map(|s| s.cpu)
        .collect();
    assert_eq!(running, [2.0]);
}

#[test]
fn samples_serialize_rates_as_whole_bytes_and_cpu_to_thousandths() {
    let mut store = Store::default();
    store.record("a", 1000, &measure(0.123_456));
    let sample = store.containers["a"].samples.since(0).next().unwrap();
    assert_eq!(
        serde_json::to_value(sample).unwrap(),
        serde_json::json!({"t": 1000, "cpu": 0.123, "memory": 536_870_912u64, "disk_read": 0, "disk_write": 4096, "net_rx": 1500, "net_tx": 1})
    );
}

#[test]
fn a_gpu_sample_marks_the_gpu_as_sampled_and_keeps_its_total() {
    let mut store = Store {
        gpu_state: GpuState::Unavailable,
        gpu_reason: Some("earlier failure".into()),
        ..Default::default()
    };
    let reading = |total| GpuReading {
        index: 1,
        name: "NVIDIA GeForce RTX 4080".into(),
        utilization: None,
        memory_used: Some(3 << 30),
        memory_total: total,
    };
    store.record_gpu(1000, &reading(Some(16 << 30)));
    store.record_gpu(2000, &reading(None));
    assert_eq!(store.gpu_state, GpuState::Sampling);
    assert_eq!(store.gpu_reason, None);
    let gpu = &store.gpus[&1];
    assert_eq!(gpu.memory_total, Some(16 << 30));
    assert_eq!(gpu.samples.len(), 2);
    assert_eq!(
        serde_json::to_value(gpu.samples.since(1000).next().unwrap()).unwrap(),
        serde_json::json!({"t": 2000, "utilization": null, "memory": 3u64 << 30})
    );
}
