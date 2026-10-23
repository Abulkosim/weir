//! Keeps macOS from napping the app while a test runs, so timers stay on time.

use tauri::AppHandle;

#[cfg(target_os = "macos")]
mod mac {
    use objc2::{rc::Retained, runtime::ProtocolObject};
    use objc2_foundation::{NSActivityOptions, NSObjectProtocol, NSProcessInfo, NSString};
    use std::cell::RefCell;

    thread_local! {
        static TOKEN: RefCell<Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>> = const { RefCell::new(None) };
    }

    pub fn begin() {
        TOKEN.with(|token| {
            let mut token = token.borrow_mut();
            if token.is_none() {
                *token = Some(
                    NSProcessInfo::processInfo().beginActivityWithOptions_reason(
                        NSActivityOptions::UserInitiated,
                        &NSString::from_str("Speed test"),
                    ),
                );
            }
        });
    }

    pub fn end() {
        TOKEN.with(|token| {
            if let Some(activity) = token.borrow_mut().take() {
                // The token came from beginActivityWithOptions on this thread.
                unsafe { NSProcessInfo::processInfo().endActivity(&activity) };
            }
        });
    }
}

pub fn begin(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.run_on_main_thread(mac::begin);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

pub fn end(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.run_on_main_thread(mac::end);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}
