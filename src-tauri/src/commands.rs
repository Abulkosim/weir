use crate::{
    core::{export::csv, history::Entry, popover::PopoverPlacement, settings::Settings, tray},
    runtime::{runtime, Start, View},
};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub fn get_popover_placement(
    visibility: State<'_, tray::Visibility>,
) -> Result<PopoverPlacement, String> {
    visibility
        .placement
        .lock()
        .map(|placement| placement.clone())
        .map_err(|_| "Popover position is unavailable.".into())
}

#[tauri::command]
pub fn fit_popover(app: AppHandle, height: f64) -> Result<(), String> {
    tray::fit(&app, height)
}

#[tauri::command]
pub fn hide_window(app: AppHandle) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or("Window is unavailable.")?
        .hide()
        .map_err(|_| "Could not close the popover.".into())
}

#[tauri::command]
pub fn get_state(app: AppHandle) -> View {
    runtime(&app).view()
}

#[tauri::command]
pub fn start_test(app: AppHandle, confirmed: bool) -> Start {
    runtime(&app).start(&app, confirmed)
}

#[tauri::command]
pub fn cancel_test(app: AppHandle) {
    runtime(&app).cancel();
}

#[tauri::command]
pub fn get_history(app: AppHandle, limit: usize) -> Vec<Entry> {
    runtime(&app).recent(limit.min(1_000))
}

#[tauri::command]
pub fn clear_history(app: AppHandle) -> Result<(), String> {
    runtime(&app).clear_history(&app)
}

/// Asks where to save, writes the CSV, and returns the file name, or `None` if cancelled.
#[tauri::command]
pub async fn export_history(app: AppHandle) -> Result<Option<String>, String> {
    let entries = runtime(&app).all_entries();
    let name = format!(
        "weir-history-{}.csv",
        chrono::Local::now().format("%Y-%m-%d")
    );
    let visibility = app.state::<tray::Visibility>();
    visibility.keep_open.store(true, Ordering::Relaxed);
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_file_name(&name)
        .add_filter("CSV", &["csv"])
        .save_file(move |path| {
            let _ = sender.send(path);
        });
    let chosen = receiver.await.ok().flatten();
    visibility.keep_open.store(false, Ordering::Relaxed);
    tray::show(&app);
    let Some(path) = chosen.and_then(|path| path.into_path().ok()) else {
        return Ok(None);
    };
    std::fs::write(&path, csv(&entries)).map_err(|_| "Weir couldn’t write the file.".to_owned())?;
    Ok(Some(
        path.file_name()
            .map_or(name, |file| file.to_string_lossy().into_owned()),
    ))
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: Settings) -> Result<View, String> {
    let runtime = runtime(&app);
    let current = runtime.view().settings;
    if current.autostart != settings.autostart {
        let launcher = app.autolaunch();
        let changed = if settings.autostart {
            launcher.enable()
        } else {
            launcher.disable()
        };
        changed.map_err(|_| "Weir couldn’t change launch at login.".to_owned())?;
    }
    let wake = settings.auto_check_updates && !current.auto_check_updates;
    let watch = settings.watch_outages != current.watch_outages;
    runtime.save_settings(&app, settings)?;
    if wake {
        app.state::<std::sync::Arc<tokio::sync::Notify>>()
            .notify_one();
    }
    if watch {
        runtime.monitor_wake.notify_one();
    }
    Ok(runtime.view())
}

#[tauri::command]
pub fn check_connection(app: AppHandle) -> bool {
    runtime(&app).check_connection(&app)
}
