use super::{
    popover::{popover_height, Bounds, PopoverPlacement},
    ring::{ring_rgba, IconStyle},
};
use std::{sync::Mutex, time::Instant};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};
#[cfg(not(target_os = "macos"))]
use {
    super::popover::layout,
    tauri_plugin_positioner::{Position, WindowExt},
};

pub const TRAY_ID: &str = "weir";

#[derive(Default)]
pub struct Visibility {
    pub last_blur: Mutex<Option<Instant>>,
    pub placement: Mutex<PopoverPlacement>,
    content_height: Mutex<Option<f64>>,
}

fn icon_style() -> IconStyle {
    if cfg!(target_os = "macos") {
        IconStyle::Template
    } else {
        taskbar_style()
    }
}

#[cfg(windows)]
fn taskbar_style() -> IconStyle {
    let light = windows_registry::CURRENT_USER
        .open(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|key| key.get_u32("SystemUsesLightTheme"))
        .is_ok_and(|value| value == 1);
    if light {
        IconStyle::LightTaskbar
    } else {
        IconStyle::DarkTaskbar
    }
}

#[cfg(not(windows))]
fn taskbar_style() -> IconStyle {
    IconStyle::DarkTaskbar
}

fn icon_size(app: &AppHandle) -> u32 {
    if cfg!(target_os = "macos") {
        return 36;
    }
    let scale = app
        .primary_monitor()
        .ok()
        .flatten()
        .map_or(1.0, |monitor| monitor.scale_factor());
    (16.0 * scale).round().clamp(16.0, 64.0) as u32
}

#[cfg(target_os = "macos")]
fn place_macos_window(app: &AppHandle, window: &tauri::WebviewWindow, height: f64) -> Option<f64> {
    use super::popover::cocoa_layout;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSScreen, NSWindow};
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    let mtm = MainThreadMarker::new()?;
    let bounds = |rect: NSRect| Bounds {
        x: rect.origin.x,
        y: rect.origin.y,
        width: rect.size.width,
        height: rect.size.height,
    };
    let pointer = window.ns_window().ok()?;
    // Keep AppKit geometry in points on the main thread, including mixed-DPI moves.
    let native = unsafe { &*pointer.cast::<NSWindow>() };
    let tray_geometry = app.tray_by_id(TRAY_ID).and_then(|tray| {
        tray.with_inner_tray_icon(move |tray| {
            let mtm = MainThreadMarker::new()?;
            let tray_window = tray.ns_status_item()?.button(mtm)?.window()?;
            let screen = tray_window.screen()?;
            Some((bounds(screen.visibleFrame()), bounds(tray_window.frame())))
        })
        .ok()
        .flatten()
    });
    let (work, anchor) = match tray_geometry {
        Some((work, anchor)) => (work, Some(anchor)),
        None => {
            let screen = native.screen().or_else(|| NSScreen::mainScreen(mtm))?;
            (bounds(screen.visibleFrame()), None)
        }
    };
    let placed = cocoa_layout(work, anchor, height);
    let content_rect = NSRect::new(
        NSPoint::new(placed.frame.x, placed.frame.y),
        NSSize::new(placed.frame.width, placed.frame.height),
    );
    native.setFrame_display(native.frameRectForContentRect(content_rect), true);
    placed.arrow_x
}

#[cfg(target_os = "macos")]
fn fit_backdrop(window: &tauri::WebviewWindow) {
    use objc2::ClassType;
    use objc2_app_kit::{NSAutoresizingMaskOptions, NSVisualEffectView, NSWindow};
    use objc2_foundation::{NSObjectProtocol, NSPoint, NSRect, NSSize};

    let window = window.clone();
    let handle = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Ok(pointer) = handle.ns_window() else {
            return;
        };
        // Tauri owns this NSWindow; access stays on its main thread while the handle lives.
        let native = unsafe { &*pointer.cast::<NSWindow>() };
        let Some(content) = native.contentView() else {
            return;
        };
        let bounds = content.bounds();
        let frame = NSRect::new(
            NSPoint::new(6.0, if content.isFlipped() { 14.0 } else { 6.0 }),
            NSSize::new(
                (bounds.size.width - 12.0).max(0.0),
                (bounds.size.height - 20.0).max(0.0),
            ),
        );
        for view in content.subviews() {
            if view.isKindOfClass(NSVisualEffectView::class()) {
                view.setAutoresizingMask(
                    NSAutoresizingMaskOptions::ViewWidthSizable
                        | NSAutoresizingMaskOptions::ViewHeightSizable,
                );
                view.setFrame(frame);
            }
        }
        native.invalidateShadow();
    });
}

pub fn show(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window("main") {
            place(&handle, &window);
            let _ = window.show();
            let _ = window.set_focus();
        }
    });
}

pub fn fit(app: &AppHandle, height: f64) -> Result<(), String> {
    let height = popover_height(Some(height));
    {
        let visibility = app.state::<Visibility>();
        let mut current = visibility
            .content_height
            .lock()
            .map_err(|_| "Popover size is unavailable.")?;
        if *current == Some(height) {
            return Ok(());
        }
        *current = Some(height);
    }
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window("main") {
            if window.is_visible().unwrap_or(false) {
                place(&handle, &window);
            }
        }
    })
    .map_err(|_| "Popover size is unavailable.".into())
}

fn place(app: &AppHandle, window: &tauri::WebviewWindow) {
    let height = popover_height(
        app.state::<Visibility>()
            .content_height
            .lock()
            .ok()
            .and_then(|value| *value),
    );
    #[cfg(target_os = "macos")]
    let arrow_x = place_macos_window(app, window, height);
    #[cfg(not(target_os = "macos"))]
    let arrow_x = {
        if window
            .move_window_constrained(Position::TrayCenter)
            .is_err()
        {
            let _ = window.center();
        }
        let anchor = app
            .tray_by_id(TRAY_ID)
            .and_then(|tray| tray.rect().ok().flatten())
            .map(|rect| {
                (
                    rect.position.to_physical::<f64>(1.0),
                    rect.size.to_physical::<f64>(1.0),
                )
            })
            .filter(|(_, size)| size.width > 0.0 && size.height > 0.0);
        let monitor = anchor
            .and_then(|(position, _)| {
                window
                    .monitor_from_point(position.x, position.y)
                    .ok()
                    .flatten()
            })
            .or_else(|| window.current_monitor().ok().flatten());
        let mut arrow_x = None;
        if let Some(monitor) = monitor {
            let work = monitor.work_area();
            let placed = layout(
                Bounds {
                    x: f64::from(work.position.x),
                    y: f64::from(work.position.y),
                    width: f64::from(work.size.width),
                    height: f64::from(work.size.height),
                },
                anchor.map(|(position, size)| Bounds {
                    x: position.x,
                    y: position.y,
                    width: size.width,
                    height: size.height,
                }),
                monitor.scale_factor(),
                false,
                height,
            );
            let resized = window.set_size(tauri::PhysicalSize::new(
                placed.frame.width as u32,
                placed.frame.height as u32,
            ));
            let positioned = window.set_position(tauri::PhysicalPosition::new(
                placed.frame.x as i32,
                placed.frame.y as i32,
            ));
            if resized.is_ok() && positioned.is_ok() {
                arrow_x = placed.arrow_x;
            }
        }
        arrow_x
    };
    if let Ok(mut placement) = app.state::<Visibility>().placement.lock() {
        placement.revision = placement.revision.saturating_add(1);
        placement.arrow_x = arrow_x;
        let _ = window.emit("popover_placement_changed", placement.clone());
    }
    #[cfg(target_os = "macos")]
    fit_backdrop(window);
}

/// Placeholder until the health ring lands: progress while testing, unknown at rest.
pub fn update(app: &AppHandle, view: &crate::runtime::View) {
    let tooltip = match (&view.testing, &view.last) {
        (Some(_), _) => "Weir · Testing".to_owned(),
        (None, Some(report)) => format!("Weir · Download {:.0} Mbps", report.download.mbps),
        (None, None) => "Weir · Not tested yet".to_owned(),
    };
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(tray) = handle.tray_by_id(TRAY_ID) {
            let _ = tray.set_tooltip(Some(&tooltip));
        }
    });
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "Open Weir", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit Weir", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&show_item, &separator, &quit_item])?;
    let size = icon_size(app);
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::new_owned(
            ring_rgba(size, None, false, icon_style()),
            size,
            size,
        ))
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("Weir · Not tested yet")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            tauri_plugin_positioner::on_tray_event(app, &event);
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let blurred = app
                    .state::<Visibility>()
                    .last_blur
                    .lock()
                    .ok()
                    .and_then(|value| *value)
                    .is_some_and(|time| time.elapsed().as_millis() < 250);
                if let Some(window) = app.get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) || blurred {
                        let _ = window.hide();
                    } else {
                        show(app);
                    }
                }
            }
        })
        .build(app)?;
    Ok(())
}
