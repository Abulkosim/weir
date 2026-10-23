use crate::{
    core::{popover::PopoverPlacement, tray},
    runtime::{runtime, View},
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
pub fn start_test(app: AppHandle) -> bool {
    runtime(&app).start(&app)
}

#[tauri::command]
pub fn cancel_test(app: AppHandle) {
    runtime(&app).cancel();
}
