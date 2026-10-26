mod activity;
mod commands;
pub mod core;
mod monitor;
mod platform;
mod runtime;
mod updates;

use crate::core::tray;
use runtime::Runtime;
use std::{sync::Arc, time::Instant};
use tauri::{Manager, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::Updates::default())
        .manage(tray::Visibility::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_popover_placement,
            commands::fit_popover,
            commands::hide_window,
            commands::get_state,
            commands::start_test,
            commands::cancel_test,
            commands::get_history,
            commands::clear_history,
            commands::export_history,
            commands::export_report,
            commands::save_settings,
            commands::check_connection,
            updates::get_update_status,
            updates::check_for_update,
            updates::install_update,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            let data = app.path().app_data_dir()?;
            let runtime = Arc::new(Runtime::new(data));
            if let Ok(enabled) = app.autolaunch().is_enabled() {
                let mut settings = runtime.view().settings;
                if settings.autostart != enabled {
                    settings.autostart = enabled;
                    let _ = runtime.save_settings(app.handle(), settings);
                }
            }
            app.manage(runtime.clone());
            let wake = Arc::new(tokio::sync::Notify::new());
            app.manage(wake.clone());
            monitor::start(app.handle().clone(), runtime.clone());
            updates::start_background_checks(app.handle().clone(), runtime, wake);
            tray::setup(app.handle())?;
            let watcher = app.handle().clone();
            platform::watch_path(move |path| runtime::runtime(&watcher).on_path(&watcher, path));
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(600)).await;
                    tray::update(&handle, &runtime::runtime(&handle).view());
                }
            });
            if !std::env::args().any(|argument| argument == "--background") {
                tray::show(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::Focused(false) => {
                let visibility = window.app_handle().state::<tray::Visibility>();
                if visibility
                    .keep_open
                    .load(std::sync::atomic::Ordering::Relaxed)
                {
                    return;
                }
                if let Ok(mut last_blur) = visibility.last_blur.lock() {
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
