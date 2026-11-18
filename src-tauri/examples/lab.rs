//! Accuracy lab runner (lab/run.ts): one speed test with the shipped engine
//! against the lab server, printed as JSON. Never part of a shipped build:
//! `WEIR_LAB=1 cargo build --profile lab --features lab --example lab --locked`.
//!
//! lab --base URL [--baseline] [--wire-iface c0]

use serde_json::{json, Value};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use weir_lib::core::{
    backends::cloudflare::Cloudflare,
    speedtest::{self, Control, Phase, Plan, Report},
    stats::{Reading, Stop, Tracker},
};

const MB: u64 = 1_000_000;
const COUNTERS: [&str; 4] = ["rx_bytes", "rx_packets", "tx_bytes", "tx_packets"];

struct Args {
    base: String,
    baseline: bool,
    wire_iface: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        base: String::new(),
        baseline: false,
        wire_iface: None,
    };
    let mut iter = std::env::args().skip(1);
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--base" => args.base = iter.next().ok_or("--base needs a URL")?,
            "--baseline" => args.baseline = true,
            "--wire-iface" => {
                args.wire_iface = Some(iter.next().ok_or("--wire-iface needs a name")?)
            }
            other => return Err(format!("unknown flag {other}")),
        }
    }
    if args.base.is_empty() {
        return Err("usage: lab --base URL [--baseline] [--wire-iface IFACE]".to_owned());
    }
    Ok(args)
}

fn unix_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |time| time.as_secs_f64() * 1_000.0)
}

fn wire(iface: Option<&str>) -> Value {
    let Some(iface) = iface else {
        return Value::Null;
    };
    let read = |name: &str| {
        std::fs::read_to_string(format!("/sys/class/net/{iface}/statistics/{name}"))
            .ok()
            .and_then(|text| text.trim().parse::<u64>().ok())
    };
    match COUNTERS.map(read) {
        [Some(rx_bytes), Some(rx_packets), Some(tx_bytes), Some(tx_packets)] => json!({
            "rxBytes": rx_bytes,
            "rxPackets": rx_packets,
            "txBytes": tx_bytes,
            "txPackets": tx_packets,
        }),
        _ => Value::Null,
    }
}

struct Row {
    unix_ms: f64,
    phase: Phase,
    phase_ms: u64,
    live_mbps: Option<f64>,
    loaded_ms: Option<f64>,
    bytes_used: u64,
    wire: Value,
}

/// The steady window the engine used for one direction, rebuilt with the
/// engine's own `Tracker` from the per-tick progress.
/// `result` is the window the engine's result came from, when it reports one
/// (upload: steady start to the first stream's last completed transfer).
fn window(
    rows: &[Row],
    phase: Phase,
    base_bytes: u64,
    capped: bool,
    result: Option<(u64, f64)>,
) -> Value {
    let mine: Vec<&Row> = rows.iter().filter(|row| row.phase == phase).collect();
    let Some(first) = mine.first() else {
        return Value::Null;
    };
    let readings: Vec<Reading> = mine
        .iter()
        .map(|row| Reading {
            at_ms: row.phase_ms,
            bytes: row.bytes_used.saturating_sub(base_bytes),
        })
        .collect();
    let tracker = Tracker::from_readings(&readings);
    let phase_start = first.unix_ms - first.phase_ms as f64;
    let stop = if capped { Stop::Cap } else { Stop::Converged };
    let rebuilt = tracker.result(stop).ok();
    let steady_start = result.map(|(start, _)| start).or_else(|| {
        rebuilt
            .as_ref()
            .map(|throughput| throughput.steady_start_ms)
            .or_else(|| tracker.steady_start().map(|(start, _)| start))
    });
    let end_ms = result.map_or(tracker.elapsed_ms() as f64, |(_, end)| end);
    json!({
        "phaseStartUnixMs": phase_start,
        "steadyStartMs": steady_start,
        "endMs": end_ms,
        "phaseEndMs": tracker.elapsed_ms(),
        "startUnixMs": steady_start.map(|start| phase_start + start as f64),
        "endUnixMs": phase_start + end_ms,
        "rebuiltMbps": rebuilt.as_ref().map(|throughput| throughput.mbps),
        "phaseBytes": tracker.bytes(),
    })
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    let mut plan = Plan::default();
    if args.baseline {
        plan.download.max_bytes = 100_000 * MB;
        plan.upload.max_bytes = 100_000 * MB;
    }
    let iface = args.wire_iface.as_deref();
    let backend = Cloudflare::with_base(&args.base);
    let control = Control::default();
    let mut rows: Vec<Row> = Vec::new();
    let wire_start = wire(iface);
    let started = Instant::now();
    let started_unix_ms = unix_ms();
    let outcome =
        tauri::async_runtime::block_on(speedtest::run(&backend, &plan, &control, |progress| {
            rows.push(Row {
                unix_ms: unix_ms(),
                phase: progress.phase,
                phase_ms: progress.phase_ms,
                live_mbps: progress.live_mbps,
                loaded_ms: progress.loaded_ms,
                bytes_used: progress.bytes_used,
                wire: wire(iface),
            });
        }));
    let wall_ms = started.elapsed().as_millis() as u64;
    let wire_end = wire(iface);
    let (report, error): (Option<Report>, Option<Value>) = match outcome {
        Ok(report) => (Some(report), None),
        Err(error) => (None, serde_json::to_value(error).ok()),
    };
    let upload_base = rows
        .iter()
        .filter(|row| row.phase == Phase::Download)
        .map(|row| row.bytes_used)
        .max()
        .unwrap_or(0);
    let capped = |phase: Phase| {
        report.as_ref().is_some_and(|report| match phase {
            Phase::Download => report.download.capped,
            _ => report.upload.as_ref().is_some_and(|speed| speed.capped),
        })
    };
    let windows = json!({
        "download": window(&rows, Phase::Download, 0, capped(Phase::Download), None),
        "upload": window(
            &rows,
            Phase::Upload,
            upload_base,
            capped(Phase::Upload),
            control.upload_window_ms.lock().ok().and_then(|window| *window),
        ),
    });
    let trace: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "unixMs": row.unix_ms,
                "phase": row.phase,
                "phaseMs": row.phase_ms,
                "liveMbps": row.live_mbps,
                "loadedMs": row.loaded_ms,
                "bytesUsed": row.bytes_used,
                "wire": row.wire,
            })
        })
        .collect();
    let output = json!({
        "report": report,
        "error": error,
        "startedUnixMs": started_unix_ms,
        "wallMs": wall_ms,
        "windows": windows,
        "wireStart": wire_start,
        "wireEnd": wire_end,
        "trace": trace,
    });
    println!("{output}");
}
