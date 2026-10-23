use crate::core::{
    backends::cloudflare::Cloudflare,
    speedtest::{self, Control, Plan, Progress, Report, TestError},
    stats::{data_estimate, DataEstimate},
};
use serde::Serialize;
use std::{
    sync::{atomic::Ordering, Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager};

pub const STATE_EVENT: &str = "state_changed";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub error: TestError,
    pub at_ms: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Estimate {
    pub up_to: bool,
    pub mb: u32,
}

impl From<DataEstimate> for Estimate {
    fn from(estimate: DataEstimate) -> Self {
        match estimate {
            DataEstimate::UpTo(mb) => Self { up_to: true, mb },
            DataEstimate::About(mb) => Self { up_to: false, mb },
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub revision: u64,
    pub testing: Option<Progress>,
    pub last: Option<Report>,
    pub failure: Option<Failure>,
    pub estimate: Estimate,
}

impl Default for View {
    fn default() -> Self {
        Self {
            revision: 0,
            testing: None,
            last: None,
            failure: None,
            estimate: data_estimate(&[]).into(),
        }
    }
}

#[derive(Default)]
pub struct Runtime {
    view: Mutex<View>,
    control: Mutex<Option<Arc<Control>>>,
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_millis() as u64)
}

impl Runtime {
    pub fn view(&self) -> View {
        self.view
            .lock()
            .map(|view| view.clone())
            .unwrap_or_default()
    }

    fn change(&self, app: &AppHandle, update: impl FnOnce(&mut View)) {
        let Ok(mut view) = self.view.lock() else {
            return;
        };
        update(&mut view);
        view.revision += 1;
        let _ = app.emit(STATE_EVENT, view.clone());
        crate::core::tray::update(app, &view);
    }

    pub fn cancel(&self) {
        if let Some(control) = self.control.lock().ok().and_then(|control| control.clone()) {
            control.cancel.store(true, Ordering::Relaxed);
        }
    }


    /// Starts a test unless one is running. Returns false when one already is.
    pub fn start(self: &Arc<Self>, app: &AppHandle) -> bool {
        let control = Arc::new(Control::default());
        {
            let Ok(mut current) = self.control.lock() else {
                return false;
            };
            if current.is_some() {
                return false;
            }
            *current = Some(control.clone());
        }
        let runtime = self.clone();
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            let outcome = speedtest::run(
                &Cloudflare::default(),
                &Plan::default(),
                &control,
                |progress| runtime.change(&handle, |view| view.testing = Some(progress)),
            )
            .await;
            if let Ok(mut current) = runtime.control.lock() {
                *current = None;
            }
            runtime.change(&handle, |view| {
                view.testing = None;
                match outcome {
                    Ok(report) => {
                        let up = report.upload.as_ref().map_or(0.0, |speed| speed.mbps);
                        view.estimate = data_estimate(&[(report.download.mbps, up)]).into();
                        view.last = Some(report);
                        view.failure = None;
                    }
                    Err(TestError::Cancelled) => {}
                    Err(error) => {
                        view.failure = Some(Failure {
                            error,
                            at_ms: unix_ms(),
                        })
                    }
                }
            });
        });
        true
    }
}

pub fn runtime(app: &AppHandle) -> Arc<Runtime> {
    app.state::<Arc<Runtime>>().inner().clone()
}
