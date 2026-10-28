//! The optional global shortcut that opens or closes the popover from any app.

use crate::core::{settings::parse_shortcut, tray};
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _, event| {
            if event.state() == ShortcutState::Pressed {
                tray::toggle(app);
            }
        })
        .build()
}

/// Swaps the registered shortcut. On failure the previous one stays registered.
pub fn replace(app: &AppHandle, previous: Option<&str>, next: Option<&str>) -> Result<(), String> {
    if previous == next {
        return Ok(());
    }
    let shortcuts = app.global_shortcut();
    if let Some(next) = next {
        shortcuts
            .register(parse_shortcut(next)?)
            .map_err(|_| "That shortcut is taken by the system or another app. Try another.")?;
    }
    if let Some(previous) = previous.and_then(|value| parse_shortcut(value).ok()) {
        let _ = shortcuts.unregister(previous);
    }
    Ok(())
}
