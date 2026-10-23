mod commands;
pub mod core;

use crate::core::tray;
use std::time::Instant;
use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            tray::show(app)
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--background"]),
        ))
        .plugin(tauri_plugin_positioner::init())
        .manage(tray::Visibility::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_popover_placement,
            commands::fit_popover,
            commands::hide_window,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            tray::setup(app.handle())?;
            if !std::env::args().any(|argument| argument == "--background") {
                tray::show(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::Focused(false) => {
                if let Ok(mut last_blur) = window
                    .app_handle()
                    .state::<tray::Visibility>()
                    .last_blur
                    .lock()
                {
                    *last_blur = Some(Instant::now());
                }
                let _ = window.hide();
            }
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("Weir could not start")
        .run(|_app, _event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event {
                tray::show(_app);
            }
        });
}
