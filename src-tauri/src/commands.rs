use crate::{
    core::{history::Entry, popover::PopoverPlacement, tray},
    runtime::{runtime, Start, View},
};
use tauri::{AppHandle, Manager, State};

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
