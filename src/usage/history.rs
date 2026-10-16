//! The samples kept in memory: a bounded ring per container and per GPU, stamped by one clock so
//! that a sample's time is also the cursor a client passes back as `since`.
use super::{gpu::GpuReading, stats::Measure};
use serde::{Serialize, Serializer};
use std::collections::{BTreeMap, HashMap, VecDeque};

/// One sample a second for 30 minutes.
pub(crate) const CAPACITY: usize = 1800;
/// Samples older than this go even while the ring has room, such as those of a stopped container.
pub(crate) const WINDOW_MS: u64 = 30 * 60 * 1000;

pub(crate) trait Stamped {
    fn t(&self) -> u64;
}

/// A series that forgets its oldest sample once it holds [`CAPACITY`].
#[derive(Debug)]
pub(crate) struct Ring<T> {
    samples: VecDeque<T>,
}

impl<T: Stamped> Ring<T> {
    /// The whole ring is reserved up front, so a full one never reallocates.
    pub(crate) fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(CAPACITY),
        }
    }
    pub(crate) fn push(&mut self, sample: T) {
        if self.samples.len() == CAPACITY {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }
    /// The samples stamped after `since`, oldest first.
    pub(crate) fn since(&self, since: u64) -> impl Iterator<Item = &T> {
        let start = self.samples.partition_point(|sample| sample.t() <= since);
        self.samples.range(start..)
    }
    /// Drop the samples stamped before `oldest`.
    pub(crate) fn prune(&mut self, oldest: u64) {
        while self.samples.front().is_some_and(|s| s.t() < oldest) {
            self.samples.pop_front();
        }
    }
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.samples.len()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// One second of a container. CPU is in cores; disk and network are bytes per second.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub(crate) struct Sample {
    /// Unix milliseconds when the manager stored it.
    pub t: u64,
    #[serde(serialize_with = "cores")]
    pub cpu: f32,
    /// Bytes in use without the page cache, as `docker stats` counts them.
    pub memory: u64,
    #[serde(serialize_with = "rate")]
    pub disk_read: f32,
    #[serde(serialize_with = "rate")]
    pub disk_write: f32,
    #[serde(serialize_with = "rate")]
    pub net_rx: f32,
    #[serde(serialize_with = "rate")]
    pub net_tx: f32,
}
impl Stamped for Sample {
    fn t(&self) -> u64 {
        self.t
    }
}

/// One second of a GPU: utilization in percent and memory in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub(crate) struct GpuSample {
    pub t: u64,
    pub utilization: Option<f32>,
    pub memory: Option<u64>,
}
impl Stamped for GpuSample {
    fn t(&self) -> u64 {
        self.t
    }
}

/// Thousandths of a core are as fine as a one-second sample can tell apart.
fn cores<S: Serializer>(value: &f32, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_f64((f64::from(*value) * 1000.0).round() / 1000.0)
}
fn rate<S: Serializer>(value: &f32, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_u64(value.max(0.0).round() as u64)
}

#[derive(Debug)]
pub(crate) struct Series {
    pub samples: Ring<Sample>,
    /// The latest limit Docker reported: the container's memory limit, or the host's memory.
    pub memory_limit: u64,
    /// The host CPUs the container sees.
    pub online_cpus: u32,
}

#[derive(Debug)]
pub(crate) struct GpuSeries {
    pub name: String,
    pub memory_total: Option<u64>,
    pub samples: Ring<GpuSample>,
}

/// Whether the GPU is being sampled, and why not.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GpuState {
    /// The installation gives desktops no GPU (`WSL_GPU` and `NVIDIA_COMPUTE` are false).
    Disabled,
    /// No GPU desktop is running, so nothing is sampled.
    #[default]
    Idle,
    Sampling,
    /// The last attempt failed; it is tried again a few minutes later.
    Unavailable,
}

#[derive(Debug, Default)]
pub(crate) struct Store {
    /// The last stamp handed out. Stamps only grow, even when the wall clock steps back.
    clock: u64,
    pub containers: HashMap<String, Series>,
    pub gpus: BTreeMap<u32, GpuSeries>,
    pub gpu_state: GpuState,
    pub gpu_reason: Option<String>,
    /// Why the running containers cannot be followed, while that lasts.
    pub error: Option<String>,
}

impl Store {
    fn stamp(&mut self, now: u64) -> u64 {
        self.clock = now.max(self.clock + 1);
        self.clock
    }
    /// The stamp of the newest sample. Every sample stamped up to it is already stored, so a
    /// client that asks for what came after it misses nothing.
    pub(crate) fn cursor(&self) -> u64 {
        self.clock
    }
    pub(crate) fn record(&mut self, id: &str, now: u64, measure: &Measure) {
        let t = self.stamp(now);
        let series = self
            .containers
            .entry(id.to_owned())
            .or_insert_with(|| Series {
                samples: Ring::new(),
                memory_limit: 0,
                online_cpus: 0,
            });
        series.memory_limit = measure.memory_limit;
        series.online_cpus = measure.online_cpus;
        series.samples.push(Sample {
            t,
            cpu: measure.cpu,
            memory: measure.memory,
            disk_read: measure.disk_read,
            disk_write: measure.disk_write,
            net_rx: measure.net_rx,
            net_tx: measure.net_tx,
        });
    }
    pub(crate) fn record_gpu(&mut self, now: u64, reading: &GpuReading) {
        let t = self.stamp(now);
        let series = self.gpus.entry(reading.index).or_insert_with(|| GpuSeries {
            name: String::new(),
            memory_total: None,
            samples: Ring::new(),
        });
        series.name.clone_from(&reading.name);
        series.memory_total = reading.memory_total.or(series.memory_total);
        series.samples.push(GpuSample {
            t,
            utilization: reading.utilization,
            memory: reading.memory_used,
        });
        self.gpu_state = GpuState::Sampling;
        self.gpu_reason = None;
    }
    /// Forget samples older than the window and every container `keep` no longer names.
    pub(crate) fn prune(&mut self, now: u64, keep: impl Fn(&str) -> bool) {
        let oldest = now.saturating_sub(WINDOW_MS);
        self.containers.retain(|id, series| {
            series.samples.prune(oldest);
            keep(id) && !series.samples.is_empty()
        });
        self.gpus.retain(|_, series| {
            series.samples.prune(oldest);
            !series.samples.is_empty()
        });
    }
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
