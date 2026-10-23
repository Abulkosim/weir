use crate::core::{
    backends::cloudflare::Cloudflare,
    history::{Entry, History, Recovery},
    meaning::{meaning, Meaning},
    network::{Network, Path},
    speedtest::{self, Control, Plan, Progress, Report, TestError},
    stats::{data_estimate, DataEstimate},
};
use serde::Serialize;
use std::{
    path::PathBuf,
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
    pub network: Network,
    pub meaning: Option<Meaning>,
    pub history_count: usize,
    pub notice: Option<String>,
}

impl Default for View {
    fn default() -> Self {
        Self {
            revision: 0,
            testing: None,
            last: None,
            failure: None,
            estimate: data_estimate(&[]).into(),
            network: Network::default(),
            meaning: None,
            history_count: 0,
            notice: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Start {
    Started,
    Running,
    NeedsConfirmation,
    Offline,
}

pub struct Runtime {
    view: Mutex<View>,
    control: Mutex<Option<Arc<Control>>>,
    path: Mutex<Option<Path>>,
    history: Mutex<History>,
}

fn notice(recovery: Recovery) -> String {
    match recovery {
        Recovery::Skipped(count) => format!(
            "{count} history {} couldn’t be read and {} skipped.",
            if count == 1 { "entry" } else { "entries" },
            if count == 1 { "was" } else { "were" }
        ),
        Recovery::BackedUp(path) => format!(
            "History couldn’t be read. It was kept as {} and a new one started.",
            path.file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
        ),
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_millis() as u64)
}

impl Runtime {
    pub fn new(history_path: PathBuf) -> Self {
        let (history, recovery) = History::load(history_path, unix_ms());
        let mut view = View {
            last: history.last_report().cloned(),
            history_count: history.len(),
            estimate: data_estimate(&history.speeds(3)).into(),
            notice: recovery.map(notice),
            ..View::default()
        };
        if let Some((at_ms, error)) = history.last_failure() {
            if view
                .last
                .as_ref()
                .is_none_or(|report| at_ms > report.started_at_ms)
            {
                view.failure = Some(Failure { error, at_ms });
            }
        }
        view.meaning = view.last.as_ref().map(meaning);
        Self {
            view: Mutex::new(view),
            control: Mutex::new(None),
            path: Mutex::new(None),
            history: Mutex::new(history),
        }
    }

    pub fn recent(&self, limit: usize) -> Vec<Entry> {
        self.history
            .lock()
            .map(|history| history.recent(limit))
            .unwrap_or_default()
    }

    pub fn clear_history(&self, app: &AppHandle) -> Result<(), String> {
        let Ok(mut history) = self.history.lock() else {
            return Err("History is unavailable.".into());
        };
        history
            .clear()
            .map_err(|_| "Weir couldn’t clear the history.".to_owned())?;
        drop(history);
        self.change(app, |view| {
            view.history_count = 0;
            view.estimate = data_estimate(&[]).into();
        });
        Ok(())
    }

    fn record(&self, entry: Entry) -> (usize, Estimate, Option<String>) {
        let Ok(mut history) = self.history.lock() else {
            return (0, data_estimate(&[]).into(), None);
        };
        let saved = history.push(entry, unix_ms());
        (
            history.len(),
            data_estimate(&history.speeds(3)).into(),
            saved
                .err()
                .map(|_| "Weir couldn’t save this test to history.".to_owned()),
        )
    }

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
        view.meaning = view.last.as_ref().map(meaning);
        view.revision += 1;
        let _ = app.emit(STATE_EVENT, view.clone());
        crate::core::tray::update(app, &view);
    }

    pub fn cancel(&self) {
        if let Some(control) = self.control.lock().ok().and_then(|control| control.clone()) {
            control.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// The OS reported a new network path. A different network mid-test aborts it.
    pub fn on_path(&self, app: &AppHandle, path: Path) {
        let previous = self
            .path
            .lock()
            .ok()
            .and_then(|mut current| current.replace(path));
        if previous.is_some_and(|previous| previous.identity() != path.identity()) {
            if let Some(control) = self.control.lock().ok().and_then(|control| control.clone()) {
                control.network_changed.store(true, Ordering::Relaxed);
            }
        }
        let next = self.view().network.next(&path, unix_ms());
        if next != self.view().network {
            self.change(app, |view| view.network = next);
        }
    }

    /// Starts a test unless one is running. A metered connection needs `confirmed`.
    pub fn start(self: &Arc<Self>, app: &AppHandle, confirmed: bool) -> Start {
        let network = self.view().network;
        if !network.online {
            return Start::Offline;
        }
        if network.metered.is_some() && !confirmed {
            return Start::NeedsConfirmation;
        }
        let control = Arc::new(Control::default());
        {
            let Ok(mut current) = self.control.lock() else {
                return Start::Running;
            };
            if current.is_some() {
                return Start::Running;
            }
            *current = Some(control.clone());
        }
        let runtime = self.clone();
        let handle = app.clone();
        crate::activity::begin(app);
        tauri::async_runtime::spawn(async move {
            let outcome = speedtest::run(
                &Cloudflare::default(),
                &Plan::default(),
                &control,
                |progress| runtime.change(&handle, |view| view.testing = Some(progress)),
            )
            .await;
            crate::activity::end(&handle);
            if let Ok(mut current) = runtime.control.lock() {
                *current = None;
            }
            let entry = match &outcome {
                Ok(report) => Some(Entry::Result {
                    report: Box::new(report.clone()),
                }),
                Err(TestError::Cancelled) => None,
                Err(error) => Some(Entry::Failed {
                    at_ms: unix_ms(),
                    error: *error,
                }),
            };
            let recorded = entry.map(|entry| runtime.record(entry));
            runtime.change(&handle, |view| {
                view.testing = None;
                if let Some((count, estimate, problem)) = recorded {
                    view.history_count = count;
                    view.estimate = estimate;
                    view.notice = problem.or(view.notice.take());
                }
                match outcome {
                    Ok(report) => {
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
        Start::Started
    }
}

pub fn runtime(app: &AppHandle) -> Arc<Runtime> {
    app.state::<Arc<Runtime>>().inner().clone()
}
