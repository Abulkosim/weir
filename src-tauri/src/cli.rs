//! Runs a command-line command and returns the exit code. It never starts the app,
//! never writes History or settings, and reads them from where the app keeps them.

use crate::core::{
    backends::cloudflare::Cloudflare,
    cli::{self, Command, USAGE},
    history::{read_only, Entry},
    speedtest::{self, Control, Plan},
    stats::{data_estimate, DataEstimate},
};
use std::{io::IsTerminal, path::PathBuf, sync::mpsc, time::Duration};

const IDENTIFIER: &str = "com.abulkosim.weir";
const OK: i32 = 0;
const FAILED: i32 = 1;
const METERED: i32 = 3;

/// The app's data folder, as Tauri resolves `app_data_dir`.
fn data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/Application Support"));
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(any(target_os = "macos", windows)))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    base.map(|base| base.join(IDENTIFIER))
}

fn entries() -> Vec<Entry> {
    data_dir()
        .map(|directory| read_only(&directory.join("history.json")))
        .unwrap_or_default()
}

fn offset() -> chrono::FixedOffset {
    *chrono::Local::now().offset()
}

/// A release build on Windows has no console of its own; borrow the terminal's.
fn attach_console() {
    #[cfg(windows)]
    // SAFETY: AttachConsole takes a process id and has no other preconditions.
    unsafe {
        let _ = windows::Win32::System::Console::AttachConsole(
            windows::Win32::System::Console::ATTACH_PARENT_PROCESS,
        );
    }
}

fn print(value: &serde_json::Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
}

fn test(allow_metered: bool) -> i32 {
    let (sender, receiver) = mpsc::channel();
    crate::platform::watch_path(move |path| {
        let _ = sender.send(path);
    });
    let path = receiver.recv_timeout(Duration::from_secs(3)).ok();
    if path.is_some_and(|path| !path.online) {
        print(&cli::failure(speedtest::TestError::Offline));
        return FAILED;
    }
    if let Some(reason) = path
        .and_then(|path| path.metered())
        .filter(|_| !allow_metered)
    {
        print(&serde_json::json!({
            "error": "metered",
            "reason": reason,
            "message": "This connection is metered or a hotspot. Run weir test --allow-metered to test anyway.",
        }));
        return METERED;
    }
    if std::io::stderr().is_terminal() {
        let history = entries();
        let mut speeds: Vec<(f64, f64)> = history
            .iter()
            .rev()
            .filter_map(Entry::report)
            .filter_map(|report| Some((report.download.mbps, report.upload.as_ref()?.mbps)))
            .take(3)
            .collect();
        speeds.reverse();
        let estimate = match data_estimate(&speeds) {
            DataEstimate::UpTo(mb) => format!("up to {mb} MB"),
            DataEstimate::About(mb) => format!("about {mb} MB"),
        };
        eprintln!("weir: testing against Cloudflare, uses {estimate}…");
    }
    let control = Control::default();
    let outcome = tauri::async_runtime::block_on(speedtest::run(
        &Cloudflare::default(),
        &Plan::default(),
        &control,
        |_| {},
    ));
    match outcome {
        Ok(report) => {
            print(&cli::result(&report, &offset()));
            OK
        }
        Err(error) => {
            print(&cli::failure(error));
            FAILED
        }
    }
}

pub fn run(command: Command) -> i32 {
    attach_console();
    match command {
        Command::Help => {
            println!("{USAGE}");
            OK
        }
        Command::Last => {
            print(&cli::last(&entries(), &offset()));
            OK
        }
        Command::History => {
            print(&cli::history(&entries(), &offset()));
            OK
        }
        Command::Test { allow_metered } => test(allow_metered),
    }
}
