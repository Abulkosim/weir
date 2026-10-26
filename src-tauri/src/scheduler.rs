//! Runs the opt-in scheduled tests about once a minute. The rules live in `core::schedule`.

use crate::{
    core::{schedule::Scheduler, settings::Schedule},
    runtime::Runtime,
};
use std::{sync::Arc, time::Duration};
use tauri::AppHandle;

const INTERVAL: Duration = Duration::from_secs(60);

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_millis() as u64)
}

pub fn start(app: AppHandle, runtime: Arc<Runtime>) {
    tauri::async_runtime::spawn(async move {
        let mut scheduler = Scheduler::default();
        loop {
            tokio::time::sleep(INTERVAL).await;
            let view = runtime.view();
            let schedule = view.settings.scheduled_tests;
            let suitable = schedule != Schedule::Off
                && view.network.online
                && view.network.metered.is_none()
                && runtime
                    .path()
                    .is_some_and(|path| path.online && path.metered().is_none())
                && view.outage_since_ms.is_none()
                && view.testing.is_none()
                && !view.checking
                && !crate::platform::low_power();
            if scheduler.tick(schedule, suitable, runtime.last_test_ms(), unix_ms()) {
                runtime.start_scheduled(&app);
            }
        }
    });
}
