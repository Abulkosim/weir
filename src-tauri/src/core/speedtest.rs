//! Runs one test: pre-flight, idle latency, download, upload (docs/method.md).

use super::{
    backends::{Backend, BackendError, Connection, Server},
    stats::{
        self, idle_latency, loaded_latency, upload_mbps, Completed, Decision, Latency, Limits,
        Probe, Reading, Stop, Throughput, Tracker,
    },
};
use serde::Serialize;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    task::JoinHandle,
    time::{sleep, Instant},
};

const MB: u64 = 1_000_000;

#[derive(Clone, Debug)]
pub struct Plan {
    pub idle_probes: usize,
    pub probe_gap: Duration,
    pub loaded_every: Duration,
    pub tick: Duration,
    pub pause: Duration,
    pub download: Limits,
    pub upload: Limits,
    pub download_streams: usize,
    pub upload_streams: usize,
    pub far_latency_ms: f64,
    pub min_request: u64,
    pub max_request: u64,
    pub download_request: Duration,
    pub upload_request: Duration,
}

impl Default for Plan {
    fn default() -> Self {
        Self {
            idle_probes: 20,
            probe_gap: Duration::from_millis(50),
            loaded_every: Duration::from_millis(250),
            tick: Duration::from_millis(100),
            pause: Duration::from_millis(300),
            download: Limits {
                min_ms: 5_000,
                max_ms: 10_000,
                max_bytes: stats::DOWNLOAD_CAP_MB as u64 * MB,
            },
            upload: Limits {
                min_ms: 5_000,
                max_ms: 8_000,
                max_bytes: stats::UPLOAD_CAP_MB as u64 * MB,
            },
            download_streams: 6,
            upload_streams: 4,
            far_latency_ms: 100.0,
            min_request: 256_000,
            max_request: 25 * MB,
            download_request: Duration::from_secs(1),
            upload_request: Duration::from_millis(500),
        }
    }
}

#[derive(Default)]
pub struct Control {
    pub cancel: AtomicBool,
    pub network_changed: AtomicBool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TestError {
    Offline,
    Refused,
    ServerError,
    Stalled,
    TooShort,
    Dropped,
    NetworkChanged,
    Cancelled,
}

impl From<stats::Failure> for TestError {
    fn from(failure: stats::Failure) -> Self {
        match failure {
            stats::Failure::TooShort => Self::TooShort,
            stats::Failure::Stalled => Self::Stalled,
            stats::Failure::Dropped => Self::Dropped,
        }
    }
}

impl From<&BackendError> for TestError {
    fn from(error: &BackendError) -> Self {
        match error {
            BackendError::Offline => Self::Offline,
            BackendError::Refused { .. } => Self::Refused,
            BackendError::ServerError { .. } => Self::ServerError,
            BackendError::Interrupted => Self::Dropped,
            BackendError::Timeout => Self::Stalled,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Connecting,
    Latency,
    Download,
    Upload,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: Phase,
    pub phase_ms: u64,
    pub live_mbps: Option<f64>,
    pub download_mbps: Option<f64>,
    pub latency_ms: Option<f64>,
    pub loaded_ms: Option<f64>,
    pub bytes_used: u64,
    pub colo: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Speed {
    pub mbps: f64,
    pub p10: f64,
    pub p90: f64,
    pub varied: bool,
    pub capped: bool,
    pub ramped_slowly: bool,
    pub rough: bool,
    pub loaded_ms: Option<f64>,
}

impl Speed {
    fn from(throughput: &Throughput, loaded_ms: Option<f64>) -> Self {
        Self {
            mbps: throughput.mbps,
            p10: throughput.p10,
            p90: throughput.p90,
            varied: throughput.varied(),
            capped: throughput.capped,
            ramped_slowly: throughput.ramped_slowly,
            rough: false,
            loaded_ms,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdleLatency {
    pub median_ms: f64,
    pub jitter_ms: Option<f64>,
    pub answered: usize,
    pub sent: usize,
}

impl From<Latency> for IdleLatency {
    fn from(latency: Latency) -> Self {
        Self {
            median_ms: latency.median_ms,
            jitter_ms: latency.jitter_ms,
            answered: latency.answered,
            sent: latency.sent,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub started_at_ms: u64,
    pub duration_ms: u64,
    pub provider: &'static str,
    pub colo: Option<String>,
    pub latency: Option<IdleLatency>,
    pub download: Speed,
    pub upload: Result<Speed, TestError>,
    pub bytes_used: u64,
}

impl Report {
    /// Latency under load: the worse of the download and upload medians.
    pub fn loaded_ms(&self) -> Option<f64> {
        let upload = self.upload.as_ref().ok().and_then(|speed| speed.loaded_ms);
        match (self.download.loaded_ms, upload) {
            (Some(down), Some(up)) => Some(down.max(up)),
            (down, up) => down.or(up),
        }
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_millis() as u64)
}

fn millis(since: Instant) -> u64 {
    since.elapsed().as_millis() as u64
}

struct Phases<'a> {
    plan: &'a Plan,
    control: &'a Control,
    used: Arc<AtomicU64>,
    colo: Option<String>,
    latency_ms: Option<f64>,
    download_mbps: Option<f64>,
}

impl Phases<'_> {
    fn check(&self) -> Result<(), TestError> {
        if self.control.cancel.load(Ordering::Relaxed) {
            Err(TestError::Cancelled)
        } else if self.control.network_changed.load(Ordering::Relaxed) {
            Err(TestError::NetworkChanged)
        } else {
            Ok(())
        }
    }

    fn progress(
        &self,
        phase: Phase,
        started: Instant,
        live: Option<f64>,
        loaded: Option<f64>,
    ) -> Progress {
        Progress {
            phase,
            phase_ms: millis(started),
            live_mbps: live,
            download_mbps: self.download_mbps,
            latency_ms: self.latency_ms,
            loaded_ms: loaded,
            bytes_used: self.used.load(Ordering::Relaxed),
            colo: self.colo.clone(),
        }
    }
}

struct Streams {
    handles: Vec<JoinHandle<()>>,
    alive: Arc<AtomicUsize>,
    error: Arc<Mutex<Option<BackendError>>>,
}

impl Drop for Streams {
    fn drop(&mut self) {
        for handle in &self.handles {
            handle.abort();
        }
    }
}

/// Size the next request so it takes about `target` at this stream's rate.
fn next_size(plan: &Plan, bytes: u64, elapsed: Duration, target: Duration) -> u64 {
    let rate = bytes as f64 / elapsed.as_secs_f64().max(0.001);
    ((rate * target.as_secs_f64()) as u64).clamp(plan.min_request, plan.max_request)
}

fn spawn_streams<C: Connection + 'static>(
    connections: &[Arc<C>],
    plan: &Plan,
    counter: &Arc<AtomicU64>,
    upload: Option<(Instant, Arc<Mutex<Vec<Completed>>>)>,
) -> Streams {
    let alive = Arc::new(AtomicUsize::new(connections.len()));
    let error = Arc::new(Mutex::new(None));
    let handles = connections
        .iter()
        .enumerate()
        .map(|(stream, connection)| {
            let (connection, counter, alive, error) = (
                connection.clone(),
                counter.clone(),
                alive.clone(),
                error.clone(),
            );
            let (plan, upload) = (plan.clone(), upload.clone());
            tokio::spawn(async move {
                let mut size = plan.min_request;
                let mut restarts = 0;
                loop {
                    let started = Instant::now();
                    let outcome = match &upload {
                        None => connection
                            .download(size, counter.clone())
                            .await
                            .map(|bytes| (bytes, None)),
                        Some(_) => connection
                            .upload(size, counter.clone())
                            .await
                            .map(|sent| (sent.bytes, Some(sent.duration_ms))),
                    };
                    match outcome {
                        Ok((bytes, duration_ms)) => {
                            if let (Some((phase, completed)), Some(duration_ms)) =
                                (&upload, duration_ms)
                            {
                                if let Ok(mut completed) = completed.lock() {
                                    completed.push(Completed {
                                        stream,
                                        started_ms: started.duration_since(*phase).as_millis()
                                            as u64,
                                        bytes,
                                        duration_ms,
                                    });
                                }
                            }
                            let target = if upload.is_some() {
                                plan.upload_request
                            } else {
                                plan.download_request
                            };
                            size = next_size(&plan, bytes, started.elapsed(), target);
                        }
                        Err(failure) => {
                            let fatal = matches!(
                                failure,
                                BackendError::Refused { .. } | BackendError::ServerError { .. }
                            );
                            if restarts == 0 && !fatal {
                                restarts += 1;
                                continue;
                            }
                            if let Ok(mut first) = error.lock() {
                                first.get_or_insert(failure);
                            }
                            alive.fetch_sub(1, Ordering::Relaxed);
                            return;
                        }
                    }
                }
            })
        })
        .collect();
    Streams {
        handles,
        alive,
        error,
    }
}

fn spawn_probes<C: Connection + 'static>(
    probe: &Arc<C>,
    every: Duration,
    phase: Instant,
) -> (JoinHandle<()>, Arc<Mutex<Vec<Probe>>>) {
    let probes = Arc::new(Mutex::new(Vec::new()));
    let (connection, sink) = (probe.clone(), probes.clone());
    let handle = tokio::spawn(async move {
        let mut next = Instant::now();
        loop {
            next += every;
            let sent_ms = millis(phase);
            let rtt_ms = connection.probe().await.ok();
            if let Ok(mut probes) = sink.lock() {
                probes.push(Probe { sent_ms, rtt_ms });
            }
            tokio::time::sleep_until(next.max(Instant::now())).await;
        }
    });
    (handle, probes)
}

struct Direction {
    throughput: Throughput,
    loaded_ms: Option<f64>,
}

async fn transfer<C: Connection + 'static>(
    phases: &Phases<'_>,
    phase: Phase,
    connections: &[Arc<C>],
    probe: &Arc<C>,
    completed: Option<Arc<Mutex<Vec<Completed>>>>,
    progress: &mut impl FnMut(Progress),
) -> Result<Direction, TestError> {
    let limits = if phase == Phase::Download {
        phases.plan.download
    } else {
        phases.plan.upload
    };
    let started = Instant::now();
    let counter = Arc::new(AtomicU64::new(0));
    let streams = spawn_streams(
        connections,
        phases.plan,
        &counter,
        completed.map(|completed| (started, completed)),
    );
    let (prober, probes) = spawn_probes(probe, phases.plan.loaded_every, started);
    let mut tracker = Tracker::new();
    let mut counted = 0;
    let outcome = loop {
        sleep(phases.plan.tick).await;
        let bytes = counter.load(Ordering::Relaxed);
        phases.used.fetch_add(bytes - counted, Ordering::Relaxed);
        counted = bytes;
        if let Err(error) = phases.check() {
            break Err(error);
        }
        tracker.push(Reading {
            at_ms: millis(started),
            bytes,
        });
        let loaded = tracker.steady_start().and_then(|(start, _)| {
            probes
                .lock()
                .ok()
                .and_then(|probes| loaded_latency(&probes, start, tracker.elapsed_ms()))
        });
        progress(phases.progress(phase, started, Some(tracker.live_mbps()), loaded));
        match tracker.decide(
            limits,
            streams.alive.load(Ordering::Relaxed),
            connections.len(),
        ) {
            Decision::Continue => {}
            Decision::Stop(stop) => break Ok(stop),
            Decision::Fail(failure) => {
                let cause = streams.error.lock().ok().and_then(|error| error.clone());
                break Err(match (failure, cause) {
                    (stats::Failure::Dropped, Some(error)) => TestError::from(&error),
                    (failure, _) => failure.into(),
                });
            }
        }
    };
    prober.abort();
    drop(streams);
    let stop: Stop = outcome?;
    let throughput = tracker.result(stop).map_err(TestError::from)?;
    let loaded_ms = probes.lock().ok().and_then(|probes| {
        loaded_latency(&probes, throughput.steady_start_ms, tracker.elapsed_ms())
    });
    Ok(Direction {
        throughput,
        loaded_ms,
    })
}

pub async fn run<B: Backend>(
    backend: &B,
    plan: &Plan,
    control: &Control,
    mut progress: impl FnMut(Progress),
) -> Result<Report, TestError> {
    let started_at_ms = unix_ms();
    let began = Instant::now();
    let mut phases = Phases {
        plan,
        control,
        used: Arc::new(AtomicU64::new(0)),
        colo: None,
        latency_ms: None,
        download_mbps: None,
    };
    progress(phases.progress(Phase::Connecting, began, None, None));
    let server: Server = backend
        .server()
        .await
        .map_err(|error| TestError::from(&error))?;
    phases.check()?;
    phases.colo = server.colo.clone();

    let latency_started = Instant::now();
    let probe = Arc::new(backend.connect().map_err(|error| TestError::from(&error))?);
    let connect = |count: usize| -> Result<Vec<Arc<B::Connection>>, TestError> {
        (0..count)
            .map(|_| {
                backend
                    .connect()
                    .map(Arc::new)
                    .map_err(|error| TestError::from(&error))
            })
            .collect()
    };
    let mut downloads = connect(plan.download_streams)?;
    let warm: Vec<_> = downloads
        .iter()
        .map(|connection| {
            let connection = connection.clone();
            tokio::spawn(async move { connection.probe().await })
        })
        .collect();
    let mut rtts = Vec::with_capacity(plan.idle_probes);
    for _ in 0..plan.idle_probes {
        phases.check()?;
        rtts.push(probe.probe().await.ok());
        if rtts.len() > 1 {
            phases.latency_ms =
                stats::median(&rtts[1..].iter().flatten().copied().collect::<Vec<_>>());
        }
        progress(phases.progress(Phase::Latency, latency_started, None, None));
        sleep(plan.probe_gap).await;
    }
    for handle in warm {
        let _ = handle.await;
    }
    let latency = idle_latency(&rtts);
    phases.latency_ms = latency.as_ref().map(|latency| latency.median_ms);
    let far = phases.latency_ms.is_some_and(|ms| ms > plan.far_latency_ms);
    if far {
        downloads.extend(connect(2)?);
    }

    let download = transfer(
        &phases,
        Phase::Download,
        &downloads,
        &probe,
        None,
        &mut progress,
    )
    .await?;
    phases.download_mbps = Some(download.throughput.mbps);
    drop(downloads);

    sleep(plan.pause).await;
    phases.check()?;
    let uploads = connect(plan.upload_streams + if far { 2 } else { 0 })?;
    let completed = Arc::new(Mutex::new(Vec::new()));
    let upload = match transfer(
        &phases,
        Phase::Upload,
        &uploads,
        &probe,
        Some(completed.clone()),
        &mut progress,
    )
    .await
    {
        Ok(direction) => {
            let mut speed = Speed::from(&direction.throughput, direction.loaded_ms);
            let requests = completed
                .lock()
                .map(|list| list.clone())
                .unwrap_or_default();
            match upload_mbps(
                &requests,
                uploads.len(),
                direction.throughput.steady_start_ms,
            ) {
                Some(mbps) => speed.mbps = mbps,
                None => speed.rough = true,
            }
            Ok(speed)
        }
        Err(error @ (TestError::Cancelled | TestError::NetworkChanged)) => return Err(error),
        Err(error) => Err(error),
    };

    Ok(Report {
        started_at_ms,
        duration_ms: millis(began),
        provider: server.provider,
        colo: server.colo,
        latency: latency.map(IdleLatency::from),
        download: Speed::from(&download.throughput, download.loaded_ms),
        upload,
        bytes_used: phases.used.load(Ordering::Relaxed),
    })
}

#[cfg(test)]
mod tests;
