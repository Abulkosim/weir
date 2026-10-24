//! The opt-in outage watch: a tiny request about once a minute on a kept-alive connection.

use crate::{
    core::backends::{cloudflare::Cloudflare, Backend, Connection},
    runtime::Runtime,
};
use std::{sync::Arc, time::Duration};
use tauri::AppHandle;

const INTERVAL: Duration = Duration::from_secs(60);
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// About a minute, give or take 10 %, so many machines do not check in step.
fn next_wait() -> Duration {
    let jitter = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.subsec_millis() as u64 % 12_000);
    INTERVAL - Duration::from_secs(6) + Duration::from_millis(jitter)
}

pub fn start(app: AppHandle, runtime: Arc<Runtime>) {
    tauri::async_runtime::spawn(async move {
        let backend = Cloudflare::default();
        let mut connection = None;
        loop {
            tokio::select! {
                _ = tokio::time::sleep(next_wait()) => {}
                _ = runtime.monitor_wake.notified() => {}
            }
            if !runtime.view().settings.watch_outages {
                connection = None;
                runtime.stop_watching(&app);
                continue;
            }
            if runtime.testing() {
                continue;
            }
            if runtime.path().is_some_and(|path| !path.online) {
                runtime.watch_tick(&app, None);
                continue;
            }
            if connection.is_none() {
                connection = backend.connect().ok();
            }
            let ok = match &connection {
                Some(connection) => tokio::time::timeout(PROBE_TIMEOUT, connection.probe())
                    .await
                    .is_ok_and(|probe| probe.is_ok()),
                None => false,
            };
            runtime.watch_tick(&app, Some(ok));
        }
    });
}
