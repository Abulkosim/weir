use super::{
    meaning::health,
    popover::{popover_height, Bounds, PopoverPlacement},
    ring::{glyph_rgba, Glyph, IconStyle},
    speedtest::{Phase, Progress},
};
use crate::runtime::View;
use serde::Serialize;
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
struct Presentation {
    pixels: Vec<u8>,
    tooltip: String,
    #[cfg(target_os = "macos")]
    title: Option<String>,
}

/// Speed text beside the icon (macOS only), from the last result.
pub fn title(view: &View, glyph: Glyph) -> Option<String> {
    let report = view.last.as_ref()?;
    (view.settings.tray_speed && matches!(glyph, Glyph::Result { .. }))
        .then(|| format!("{:.0}", report.download.mbps))
}

#[cfg(target_os = "macos")]
fn set_tabular_title(tray: &tauri::tray::TrayIcon, title: Option<&str>) -> bool {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSFont, NSFontWeightRegular};

    if tray.set_title(title).is_err() {
        return false;
    }
    let _ = tray.with_inner_tray_icon(|tray| {
        let mtm = MainThreadMarker::new()?;
        let button = tray.ns_status_item()?.button(mtm)?;
        let size = NSFont::menuBarFontOfSize(0.0).pointSize();
        let font =
            NSFont::monospacedDigitSystemFontOfSize_weight(size, unsafe { NSFontWeightRegular });
        button.setFont(Some(&font));
        Some(())
    });
    true
}

#[derive(Default)]
pub struct Visibility {
    presentation: Mutex<Presentation>,
    /// Last rendered glyph and size; a test emits progress ten times a second
    /// but the gauge only changes in 1/36 steps.
    rendered: Mutex<Option<(Glyph, u32, Vec<u8>)>>,
    /// Set while a native dialog is open so losing focus does not hide the popover.
    pub keep_open: std::sync::atomic::AtomicBool,
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

const STALE_MS: u64 = 24 * 60 * 60 * 1000;

/// Overall progress through the three phases, 0–1.
pub fn progress_fraction(progress: &Progress) -> f64 {
    let ms = progress.phase_ms as f64;
    let (start, share, span) = match progress.phase {
        Phase::Connecting => (0.0, 0.0, 1.0),
        Phase::Latency => (0.0, 0.1, 1_500.0),
        Phase::Download => (0.1, 0.5, 10_000.0),
        Phase::Upload => (0.6, 0.4, 8_000.0),
    };
    let fraction = start + share * (ms / span).min(1.0);
    (fraction * 36.0).round() / 36.0
}

pub fn glyph(view: &View, now_ms: u64) -> Glyph {
    if let Some(progress) = &view.testing {
        return Glyph::Testing(progress_fraction(progress));
    }
    if !view.network.online || view.outage_since_ms.is_some() {
        return Glyph::Offline;
    }
    let failed_last = view.failure.as_ref().is_some_and(|failure| {
        view.last
            .as_ref()
            .is_none_or(|report| failure.at_ms > report.started_at_ms)
    });
    match &view.last {
        _ if failed_last => Glyph::Failed,
        Some(report) => Glyph::Result {
            health: health(report),
            stale: now_ms.saturating_sub(report.started_at_ms) > STALE_MS,
        },
        None => Glyph::Unknown,
    }
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

/// The last result at a glance, without opening the popover.
pub fn tooltip(view: &View, glyph: Glyph) -> String {
    let summary = |report: &crate::core::speedtest::Report| {
        let mut line = format!("Download {:.0} Mbps", report.download.mbps);
        if let Some(upload) = &report.upload {
            line.push_str(&format!(" · Upload {:.0} Mbps", upload.mbps));
        }
        if let Some(latency) = &report.latency {
            line.push_str(&format!(" · {:.0} ms", latency.median_ms));
        }
        line
    };
    match (glyph, &view.last) {
        (Glyph::Testing(_), _) => "Weir · Testing".into(),
        (Glyph::Offline, _) => match view.network.offline_since_ms.or(view.outage_since_ms) {
            Some(since) => format!("Weir · No internet since {}", clock(since)),
            None => "Weir · No internet".into(),
        },
        (Glyph::Failed, _) => "Weir · Last test failed".into(),
        (Glyph::Result { stale, .. }, Some(report)) => format!(
            "Weir · {}\nTested {}{}",
            summary(report),
            clock(report.started_at_ms),
            if stale { ", over a day ago" } else { "" }
        ),
        _ => "Weir · Not tested yet".into(),
    }
}

pub fn update(app: &AppHandle, view: &View) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_millis() as u64);
    let glyph = glyph(view, now_ms);
    let size = icon_size(app);
    let visibility = app.state::<Visibility>();
    let cached = visibility.rendered.lock().ok().and_then(|rendered| {
        rendered
            .as_ref()
            .filter(|(shown, shown_size, _)| *shown == glyph && *shown_size == size)
            .map(|(_, _, pixels)| pixels.clone())
    });
    let Some(pixels) = cached.or_else(|| {
        let pixels = glyph_rgba(size, glyph, icon_style());
        if let Ok(mut rendered) = visibility.rendered.lock() {
            *rendered = Some((glyph, size, pixels.clone()));
        }
        Some(pixels)
    }) else {
        return;
    };
    let tooltip = tooltip(view, glyph);
    #[cfg(target_os = "macos")]
    let title = title(view, glyph);
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(tray) = handle.tray_by_id(TRAY_ID) else {
            return;
        };
        let visibility = handle.state::<Visibility>();
        let Ok(mut shown) = visibility.presentation.lock() else {
            return;
        };
        if shown.pixels != pixels
            && tray
                .set_icon_with_as_template(
                    Some(Image::new_owned(pixels.clone(), size, size)),
                    cfg!(target_os = "macos"),
                )
                .is_ok()
        {
            shown.pixels = pixels;
        }
        if shown.tooltip != tooltip && tray.set_tooltip(Some(&tooltip)).is_ok() {
            shown.tooltip = tooltip;
        }
        #[cfg(target_os = "macos")]
        if shown.title != title && set_tabular_title(&tray, title.as_deref()) {
            shown.title = title;
        }
    });
}

/// What the popover should show when opened from the tray menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Navigate {
    Overview,
    Confirm,
    Check,
    History,
    Settings,
}

pub const NAVIGATE_EVENT: &str = "navigate";

pub fn navigate(app: &AppHandle, target: Navigate) {
    show(app);
    let _ = app.emit(NAVIGATE_EVENT, target);
}

/// Runs a test straight from the menu. The popover opens only when it has
/// something to ask or say: a metered link, no internet, or a test already running.
fn menu_test(app: &AppHandle) {
    use crate::runtime::{runtime, Start};
    match runtime(app).start(app, false) {
        Start::Started => {}
        Start::NeedsConfirmation => navigate(app, Navigate::Confirm),
        Start::Offline | Start::Running => navigate(app, Navigate::Overview),
    }
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let test_item = MenuItem::with_id(app, "test", "Run speed test", true, None::<&str>)?;
    let check_item = MenuItem::with_id(app, "check", "Check connection", true, None::<&str>)?;
    let history_item = MenuItem::with_id(app, "history", "History", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit Weir", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let separator_two = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &test_item,
            &check_item,
            &separator,
            &history_item,
            &settings_item,
            &separator_two,
            &quit_item,
        ],
    )?;
    let size = icon_size(app);
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::new_owned(
            glyph_rgba(size, Glyph::Unknown, icon_style()),
            size,
            size,
        ))
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("Weir · Not tested yet")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "test" => menu_test(app),
            "check" => navigate(app, Navigate::Check),
            "history" => navigate(app, Navigate::History),
            "settings" => navigate(app, Navigate::Settings),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        core::speedtest::{IdleLatency, Report, Speed},
        runtime::Failure,
    };

    fn progress(phase: Phase, phase_ms: u64) -> Progress {
        Progress {
            phase,
            phase_ms,
            live_mbps: None,
            download_mbps: None,
            latency_ms: None,
            loaded_ms: None,
            bytes_used: 0,
            colo: None,
        }
    }

    fn report(started_at_ms: u64) -> Report {
        let speed = |mbps| Speed {
            mbps,
            p10: mbps,
            p90: mbps,
            varied: false,
            capped: false,
            ramped_slowly: false,
            rough: false,
            loaded_ms: Some(40.0),
        };
        Report {
            started_at_ms,
            duration_ms: 15_000,
            provider: "Cloudflare".into(),
            colo: None,
            latency: Some(IdleLatency {
                median_ms: 12.0,
                jitter_ms: Some(1.0),
                answered: 19,
                sent: 19,
            }),
            download: speed(300.0),
            upload: Some(speed(40.0)),
            upload_error: None,
            bytes_used: 0,
            scheduled: false,
        }
    }

    #[test]
    fn progress_moves_forward_through_the_phases() {
        let steps = [
            progress(Phase::Connecting, 300),
            progress(Phase::Latency, 750),
            progress(Phase::Download, 0),
            progress(Phase::Download, 5_000),
            progress(Phase::Upload, 0),
            progress(Phase::Upload, 8_000),
        ];
        let fractions: Vec<f64> = steps.iter().map(progress_fraction).collect();
        assert!(
            fractions.windows(2).all(|pair| pair[0] <= pair[1]),
            "{fractions:?}"
        );
        assert_eq!(fractions[0], 0.0);
        assert_eq!(fractions[5], 1.0);
    }

    #[test]
    fn glyph_follows_testing_then_result_then_age() {
        let now = 10 * STALE_MS;
        let mut view = View::default();
        assert_eq!(glyph(&view, now), Glyph::Unknown);
        view.testing = Some(progress(Phase::Download, 5_000));
        assert!(matches!(glyph(&view, now), Glyph::Testing(_)));
        view.testing = None;
        view.last = Some(report(now - 60_000));
        assert_eq!(
            glyph(&view, now),
            Glyph::Result {
                health: crate::core::meaning::Health::Good,
                stale: false
            }
        );
        view.last = Some(report(now - STALE_MS - 1));
        assert!(matches!(
            glyph(&view, now),
            Glyph::Result { stale: true, .. }
        ));
        view.failure = Some(Failure {
            error: crate::core::speedtest::TestError::Stalled,
            at_ms: now - 1_000,
        });
        assert_eq!(glyph(&view, now), Glyph::Failed);
        view.last = Some(report(now));
        assert!(matches!(glyph(&view, now), Glyph::Result { .. }));
        view.network.online = false;
        assert_eq!(glyph(&view, now), Glyph::Offline);
    }

    #[test]
    fn tooltip_summarises_the_last_result() {
        let view = View {
            last: Some(report(0)),
            ..View::default()
        };
        let glyph = Glyph::Result {
            health: crate::core::meaning::Health::Good,
            stale: false,
        };
        let text = tooltip(&view, glyph);
        assert!(
            text.starts_with("Weir · Download 300 Mbps · Upload 40 Mbps · 12 ms\nTested "),
            "{text}"
        );
        assert_eq!(
            tooltip(&View::default(), Glyph::Unknown),
            "Weir · Not tested yet"
        );
        assert_eq!(tooltip(&view, Glyph::Testing(0.3)), "Weir · Testing");
    }

    #[test]
    fn speed_text_only_when_enabled_and_showing_a_result() {
        let mut view = View {
            last: Some(report(0)),
            ..View::default()
        };
        let shown = Glyph::Result {
            health: crate::core::meaning::Health::Good,
            stale: false,
        };
        assert_eq!(title(&view, shown), None);
        view.settings.tray_speed = true;
        assert_eq!(title(&view, shown).as_deref(), Some("300"));
        assert_eq!(title(&view, Glyph::Offline), None);
        assert_eq!(title(&view, Glyph::Testing(0.5)), None);
    }
}
