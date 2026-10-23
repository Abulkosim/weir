use super::*;
use crate::core::backends::Sent;
use std::future::pending;

#[derive(Clone)]
struct Fake {
    down_mbps: f64,
    up_mbps: f64,
    rtt_ms: f64,
    bloat_ms: f64,
    stall_after: Option<Duration>,
    refuse_upload: bool,
    offline: bool,
    active: Arc<AtomicUsize>,
    connections: Arc<AtomicUsize>,
}

impl Default for Fake {
    fn default() -> Self {
        Self {
            down_mbps: 50.0,
            up_mbps: 10.0,
            rtt_ms: 15.0,
            bloat_ms: 45.0,
            stall_after: None,
            refuse_upload: false,
            offline: false,
            active: Arc::new(AtomicUsize::new(0)),
            connections: Arc::new(AtomicUsize::new(0)),
        }
    }
}

struct FakeConnection {
    fake: Fake,
    first: Mutex<Option<Instant>>,
}

impl FakeConnection {
    async fn pace(&self, bytes: u64, mbps: f64, counter: &AtomicU64) {
        let first = *self.first.lock().unwrap().get_or_insert_with(Instant::now);
        let mut sent = 0;
        while sent < bytes {
            let chunk = (bytes - sent).min(64 * 1024);
            let t = first.elapsed();
            if self.fake.stall_after.is_some_and(|after| t >= after) {
                pending::<()>().await;
            }
            let ramp = (1.0 - (-t.as_secs_f64() / 0.5).exp()).max(0.05);
            sleep(Duration::from_secs_f64(
                chunk as f64 * 8.0 / (mbps * 1e6 * ramp),
            ))
            .await;
            counter.fetch_add(chunk, Ordering::Relaxed);
            sent += chunk;
        }
    }
}

struct Active<'a>(&'a AtomicUsize);

impl<'a> Active<'a> {
    fn new(count: &'a AtomicUsize) -> Self {
        count.fetch_add(1, Ordering::Relaxed);
        Self(count)
    }
}

impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

impl Connection for FakeConnection {
    async fn probe(&self) -> Result<f64, BackendError> {
        let loaded = self.fake.active.load(Ordering::Relaxed) > 0;
        let rtt = self.fake.rtt_ms + if loaded { self.fake.bloat_ms } else { 0.0 };
        sleep(Duration::from_secs_f64(rtt / 1_000.0)).await;
        Ok(rtt)
    }

    async fn download(&self, bytes: u64, counter: Arc<AtomicU64>) -> Result<u64, BackendError> {
        let _active = Active::new(&self.fake.active);
        self.pace(bytes, self.fake.down_mbps, &counter).await;
        Ok(bytes)
    }

    async fn upload(&self, bytes: u64, counter: Arc<AtomicU64>) -> Result<Sent, BackendError> {
        if self.fake.refuse_upload {
            return Err(BackendError::Refused {
                status: 403,
                retry_after: None,
            });
        }
        let _active = Active::new(&self.fake.active);
        let started = Instant::now();
        self.pace(bytes, self.fake.up_mbps, &counter).await;
        sleep(Duration::from_secs_f64(self.fake.rtt_ms / 1_000.0)).await;
        Ok(Sent {
            bytes,
            duration_ms: started.elapsed().as_secs_f64() * 1_000.0,
        })
    }
}

impl Backend for Fake {
    type Connection = FakeConnection;

    async fn server(&self) -> Result<Server, BackendError> {
        if self.offline {
            return Err(BackendError::Offline);
        }
        Ok(Server {
            provider: "Cloudflare",
            colo: Some("FRA".into()),
        })
    }

    fn connect(&self) -> Result<FakeConnection, BackendError> {
        self.connections.fetch_add(1, Ordering::Relaxed);
        Ok(FakeConnection {
            fake: self.clone(),
            first: Mutex::new(None),
        })
    }
}

fn close(actual: f64, expected: f64, tolerance: f64) -> bool {
    (actual - expected).abs() <= expected * tolerance
}

#[tokio::test(start_paused = true)]
async fn full_test_reports_every_metric() {
    let fake = Fake::default();
    let mut events = Vec::new();
    let report = run(&fake, &Plan::default(), &Control::default(), |progress| {
        events.push(progress)
    })
    .await
    .unwrap();

    assert!(
        close(report.download.mbps, 300.0, 0.05),
        "{}",
        report.download.mbps
    );
    let upload = report.upload.as_ref().unwrap();
    assert!(close(upload.mbps, 40.0, 0.1), "{}", upload.mbps);
    assert!(!upload.rough);
    let latency = report.latency.as_ref().unwrap();
    assert!(close(latency.median_ms, 15.0, 0.01));
    assert_eq!(latency.sent, 19);
    assert!(close(report.loaded_ms().unwrap(), 60.0, 0.01));
    assert_eq!(report.colo.as_deref(), Some("FRA"));
    assert_eq!(report.provider, "Cloudflare");
    assert!(
        report.bytes_used > 150 * MB && report.bytes_used < 600 * MB,
        "{} {}",
        report.bytes_used,
        report.duration_ms
    );
    assert!(report.duration_ms < 21_000, "{}", report.duration_ms);

    let phases: Vec<Phase> = events.iter().map(|event| event.phase).collect();
    for phase in [
        Phase::Connecting,
        Phase::Latency,
        Phase::Download,
        Phase::Upload,
    ] {
        assert!(phases.contains(&phase), "{phase:?}");
    }
    assert!(phases.windows(2).all(|pair| pair[0] as u8 <= pair[1] as u8));
    assert!(events
        .iter()
        .filter(|event| event.phase == Phase::Upload)
        .all(|event| event.download_mbps == Some(report.download.mbps)));
    assert!(events
        .windows(2)
        .all(|pair| pair[0].bytes_used <= pair[1].bytes_used));
}

#[tokio::test(start_paused = true)]
async fn stalled_download_fails_without_numbers() {
    let fake = Fake {
        stall_after: Some(Duration::from_millis(2_500)),
        ..Fake::default()
    };
    let result = run(&fake, &Plan::default(), &Control::default(), |_| {}).await;
    assert_eq!(result, Err(TestError::Stalled));
}

#[tokio::test(start_paused = true)]
async fn cancel_mid_download_leaves_no_result() {
    let fake = Fake::default();
    let control = Control::default();
    let result = run(&fake, &Plan::default(), &control, |progress| {
        if progress.phase == Phase::Download && progress.phase_ms > 2_000 {
            control.cancel.store(true, Ordering::Relaxed);
        }
    })
    .await;
    assert_eq!(result, Err(TestError::Cancelled));
}

#[tokio::test(start_paused = true)]
async fn network_change_aborts_the_test() {
    let fake = Fake::default();
    let control = Control::default();
    let result = run(&fake, &Plan::default(), &control, |progress| {
        if progress.phase == Phase::Upload {
            control.network_changed.store(true, Ordering::Relaxed);
        }
    })
    .await;
    assert_eq!(result, Err(TestError::NetworkChanged));
}

#[tokio::test(start_paused = true)]
async fn offline_stops_before_any_transfer() {
    let fake = Fake {
        offline: true,
        ..Fake::default()
    };
    let result = run(&fake, &Plan::default(), &Control::default(), |_| {}).await;
    assert_eq!(result, Err(TestError::Offline));
    assert_eq!(fake.connections.load(Ordering::Relaxed), 0);
}

#[tokio::test(start_paused = true)]
async fn refused_upload_keeps_the_download_and_says_why() {
    let fake = Fake {
        refuse_upload: true,
        ..Fake::default()
    };
    let report = run(&fake, &Plan::default(), &Control::default(), |_| {})
        .await
        .unwrap();
    assert!(close(report.download.mbps, 300.0, 0.05));
    assert_eq!(report.upload, Err(TestError::Refused));
}

#[tokio::test(start_paused = true)]
async fn distant_server_gets_more_streams() {
    let fake = Fake {
        rtt_ms: 150.0,
        ..Fake::default()
    };
    let report = run(&fake, &Plan::default(), &Control::default(), |_| {})
        .await
        .unwrap();
    assert_eq!(fake.connections.load(Ordering::Relaxed), 1 + 8 + 6);
    assert!(
        close(report.download.mbps, 400.0, 0.06),
        "{}",
        report.download.mbps
    );
}
