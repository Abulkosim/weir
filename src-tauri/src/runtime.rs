use crate::core::{
    backends::cloudflare::Cloudflare,
    diagnose::{diagnose, Diagnosis},
    history::{Entry, History, Recovery},
    meaning::{meaning, Meaning},
    network::{Network, Path},
    outage::{Event, Watch},
    plan::{compare, Comparison},
    settings::{Settings, Store},
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
    /// The last result against the speed the provider sells, when it is set.
    pub plan: Option<Comparison>,
    pub history_count: usize,
    pub notice: Option<String>,
    pub settings: Settings,
    pub checking: bool,
    pub diagnosis: Option<Diagnosis>,
    /// Start of an outage found by the opt-in watch, while it lasts.
    pub outage_since_ms: Option<u64>,
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
            plan: None,
            history_count: 0,
            notice: None,
            settings: Settings::default(),
            checking: false,
            diagnosis: None,
            outage_since_ms: None,
        }
    }
}

impl View {
    /// Fields that follow from the last result and the settings.
    fn derive(&mut self) {
        self.meaning = self.last.as_ref().map(meaning);
        self.plan = self
            .last
            .as_ref()
            .and_then(|report| compare(report, self.settings.plan()));
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
    store: Store,
    watch: Mutex<Watch>,
    pub monitor_wake: tokio::sync::Notify,
}

fn clock(ms: u64) -> String {
    chrono::DateTime::from_timestamp_millis(ms as i64)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%H:%M")
                .to_string()
        })
        .unwrap_or_default()
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
    pub fn new(data: PathBuf) -> Self {
        let (history, recovery) = History::load(data.join("history.json"), unix_ms());
        let (store, settings, mut notes) = Store::load(data.join("settings.json"), unix_ms());
        notes.extend(recovery.map(notice));
        let mut view = View {
            last: history.last_report().cloned(),
            history_count: history.len(),
            estimate: data_estimate(&history.speeds(3)).into(),
            notice: (!notes.is_empty()).then(|| notes.join(" ")),
            settings,
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
        view.derive();
        Self {
            view: Mutex::new(view),
            control: Mutex::new(None),
            path: Mutex::new(None),
            history: Mutex::new(history),
            store,
            watch: Mutex::new(Watch::default()),
            monitor_wake: tokio::sync::Notify::new(),
        }
    }

    fn notify(app: &AppHandle, title: &str, body: &str) {
        use tauri_plugin_notification::NotificationExt;
        let _ = app.notification().builder().title(title).body(body).show();
    }

    /// Applies what the outage watch decided: view, history and notifications.
    pub fn outage_events(&self, app: &AppHandle, events: Vec<Event>) {
        let notify = self.view().settings.notify_outages;
        for event in events {
            match event {
                Event::Lost { since_ms } => {
                    self.change(app, |view| view.outage_since_ms = Some(since_ms));
                    if notify {
                        Self::notify(
                            app,
                            "Connection lost",
                            &format!("No internet since {}.", clock(since_ms)),
                        );
                    }
                }
                Event::Back { start_ms, end_ms } => {
                    let (count, _, problem) = self.record(Entry::Outage { start_ms, end_ms });
                    self.change(app, |view| {
                        view.outage_since_ms = None;
                        view.history_count = count;
                        view.notice = problem.or(view.notice.take());
                    });
                    if notify {
                        let minutes = end_ms.saturating_sub(start_ms).div_ceil(60_000);
                        Self::notify(
                            app,
                            "Connection back",
                            &format!(
                                "No internet {}–{} ({minutes} min).",
                                clock(start_ms),
                                clock(end_ms)
                            ),
                        );
                    }
                }
            }
        }
    }

    /// One tick of the opt-in watch. `ok` is `None` when the OS reports no path.
    pub fn watch_tick(&self, app: &AppHandle, ok: Option<bool>) {
        let now = unix_ms();
        let events = match self.watch.lock() {
            Ok(mut watch) => match ok {
                Some(ok) => watch.on_check(ok, now),
                None => watch.on_offline(now),
            },
            Err(_) => return,
        };
        self.outage_events(app, events);
    }

    pub fn stop_watching(&self, app: &AppHandle) {
        if let Ok(mut watch) = self.watch.lock() {
            watch.reset();
        }
        if self.view().outage_since_ms.is_some() {
            self.change(app, |view| view.outage_since_ms = None);
        }
    }

    pub fn testing(&self) -> bool {
        self.control
            .lock()
            .map(|control| control.is_some())
            .unwrap_or(false)
    }

    pub fn path(&self) -> Option<Path> {
        self.path.lock().ok().and_then(|path| *path)
    }

    /// Runs the connection check unless one is already running.
    pub fn check_connection(self: &Arc<Self>, app: &AppHandle) -> bool {
        if self.view().checking {
            return false;
        }
        self.change(app, |view| view.checking = true);
        let path = self
            .path
            .lock()
            .ok()
            .and_then(|path| *path)
            .unwrap_or(Path {
                online: true,
                ..Path::default()
            });
        let runtime = self.clone();
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            let readings = crate::core::probe::readings(&Cloudflare::default(), path).await;
            let diagnosis = diagnose(&readings, unix_ms());
            runtime.change(&handle, |view| {
                view.checking = false;
                view.diagnosis = Some(diagnosis);
            });
        });
        true
    }

    pub fn save_settings(&self, app: &AppHandle, mut settings: Settings) -> Result<(), String> {
        let plan = settings.plan();
        settings.plan_download_mbps = plan.download_mbps;
        settings.plan_upload_mbps = plan.upload_mbps;
        let saved = self.store.save(&settings);
        self.change(app, |view| view.settings = settings);
        saved.map_err(|_| "Weir couldn’t save settings.".to_owned())
    }

    pub fn all_entries(&self) -> Vec<Entry> {
        self.history
            .lock()
            .map(|history| history.entries().to_vec())
            .unwrap_or_default()
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
        view.derive();
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
        if !path.online && self.view().settings.watch_outages {
            self.watch_tick(app, None);
        } else if path.online && previous.is_some_and(|previous| !previous.online) {
            self.monitor_wake.notify_one();
        }
        let next = self.view().network.next(&path, unix_ms());
        if next != self.view().network {
            self.change(app, |view| view.network = next);
        }
    }

    /// The newest test in history, by hand or scheduled, passed or failed.
    pub fn last_test_ms(&self) -> Option<u64> {
        self.history.lock().ok().and_then(|history| {
            history
                .entries()
                .iter()
                .rev()
                .find(|entry| !matches!(entry, Entry::Outage { .. }))
                .map(Entry::at_ms)
        })
    }

    /// Starts a test unless one is running. A metered connection needs `confirmed`.
    pub fn start(self: &Arc<Self>, app: &AppHandle, confirmed: bool) -> Start {
        self.run(app, confirmed, false)
    }

    /// A test from Settings → Scheduled tests. It never confirms a metered connection,
    /// and a failure goes to history without taking over the popover.
    pub fn start_scheduled(self: &Arc<Self>, app: &AppHandle) -> Start {
        self.run(app, false, true)
    }

    fn run(self: &Arc<Self>, app: &AppHandle, confirmed: bool, scheduled: bool) -> Start {
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
            let outcome = outcome.map(|report| Report {
                scheduled,
                ..report
            });
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
                    Err(_) if scheduled => {}
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
