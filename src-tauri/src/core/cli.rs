//! `weir last`, `weir history` and `weir test` for scripts. Every command prints JSON with
//! the same fields as the CSV export. Fields: README "Command line".

use super::{
    export::{error_text, notes},
    history::Entry,
    meaning::meaning,
    speedtest::{Report, TestError},
};
use chrono::{DateTime, FixedOffset, SecondsFormat};
use serde_json::{json, Value};

pub const USAGE: &str =
    "Usage: weir --json | weir last | weir history | weir test [--allow-metered]

  last      the last speed test (also: weir --json)
  history   every test and outage in History, oldest first
  test      runs a full test now; asks for --allow-metered on a hotspot or metered link

Every command prints JSON and leaves History unchanged.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Last,
    History,
    Test { allow_metered: bool },
}

/// `None` when the arguments ask for the app itself, as at login (`--background`).
pub fn parse(args: &[String]) -> Result<Option<Command>, String> {
    let mut json = false;
    let mut allow_metered = false;
    let mut words = Vec::new();
    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "--json" => json = true,
            "--allow-metered" => allow_metered = true,
            "--help" | "-h" | "help" => return Ok(Some(Command::Help)),
            "--background" => {}
            other if other.starts_with("-psn") => {}
            other if other.starts_with('-') => return Err(USAGE.into()),
            other => words.push(other),
        }
    }
    let command = match words.as_slice() {
        [] if json => Command::Last,
        [] => return Ok(None),
        ["last"] => Command::Last,
        ["history"] => Command::History,
        ["test"] => Command::Test { allow_metered },
        _ => return Err(USAGE.into()),
    };
    if allow_metered && !matches!(command, Command::Test { .. }) {
        return Err(USAGE.into());
    }
    Ok(Some(command))
}

fn timestamp(ms: u64, offset: &FixedOffset) -> String {
    DateTime::from_timestamp_millis(ms as i64)
        .unwrap_or_default()
        .with_timezone(offset)
        .to_rfc3339_opts(SecondsFormat::Secs, false)
}

fn tenths(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

pub fn result(report: &Report, offset: &FixedOffset) -> Value {
    let verdict = meaning(report);
    json!({
        "tested_at": timestamp(report.started_at_ms, offset),
        "result": "ok",
        "server": format!("{} {}", report.provider, report.colo.as_deref().unwrap_or("")).trim_end(),
        "download_mbps": tenths(report.download.mbps),
        "upload_mbps": report.upload.as_ref().map(|speed| tenths(speed.mbps)),
        "latency_ms": report.latency.as_ref().map(|latency| tenths(latency.median_ms)),
        "latency_under_load_ms": report.loaded_ms().map(tenths),
        "jitter_ms": report.latency.as_ref().and_then(|latency| latency.jitter_ms).map(tenths),
        "download_low_mbps": tenths(report.download.p10),
        "download_high_mbps": tenths(report.download.p90),
        "data_used_mb": tenths(report.bytes_used as f64 / 1_000_000.0),
        "scheduled": report.scheduled,
        "health": verdict.health,
        "verdict": verdict.headline,
        "notes": notes(report),
    })
}

pub fn failure(error: TestError) -> Value {
    json!({ "error": error, "message": error_text(error) })
}

pub fn entry(entry: &Entry, offset: &FixedOffset) -> Value {
    match entry {
        Entry::Result { report } => result(report, offset),
        Entry::Failed { at_ms, error } => json!({
            "tested_at": timestamp(*at_ms, offset),
            "result": "failed",
            "error": error,
            "message": error_text(*error),
        }),
        Entry::Outage { start_ms, end_ms } => json!({
            "tested_at": timestamp(*start_ms, offset),
            "result": "outage",
            "until": timestamp(*end_ms, offset),
            "minutes": end_ms.saturating_sub(*start_ms).div_ceil(60_000),
        }),
    }
}

/// The newest result, or `null` before the first test.
pub fn last(entries: &[Entry], offset: &FixedOffset) -> Value {
    entries
        .iter()
        .rev()
        .find_map(Entry::report)
        .map_or(Value::Null, |report| result(report, offset))
}

pub fn history(entries: &[Entry], offset: &FixedOffset) -> Value {
    Value::Array(entries.iter().map(|item| entry(item, offset)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::speedtest::{IdleLatency, Speed};

    fn args(words: &[&str]) -> Vec<String> {
        std::iter::once("weir")
            .chain(words.iter().copied())
            .map(String::from)
            .collect()
    }

    #[test]
    fn parses_commands() {
        assert_eq!(parse(&args(&[])), Ok(None));
        assert_eq!(parse(&args(&["--background"])), Ok(None));
        assert_eq!(parse(&args(&["-psn_0_12345"])), Ok(None));
        assert_eq!(parse(&args(&["--json"])), Ok(Some(Command::Last)));
        assert_eq!(parse(&args(&["last"])), Ok(Some(Command::Last)));
        assert_eq!(
            parse(&args(&["history", "--json"])),
            Ok(Some(Command::History))
        );
        assert_eq!(
            parse(&args(&["test"])),
            Ok(Some(Command::Test {
                allow_metered: false
            }))
        );
        assert_eq!(
            parse(&args(&["--json", "test", "--allow-metered"])),
            Ok(Some(Command::Test {
                allow_metered: true
            }))
        );
        assert_eq!(parse(&args(&["--help"])), Ok(Some(Command::Help)));
        assert!(parse(&args(&["last", "--allow-metered"])).is_err());
        assert!(parse(&args(&["speed"])).is_err());
        assert!(parse(&args(&["test", "now"])).is_err());
        assert!(parse(&args(&["--verbose"])).is_err());
    }

    fn report() -> Report {
        let speed = |mbps, loaded_ms| Speed {
            mbps,
            p10: mbps * 0.95,
            p90: mbps * 1.05,
            varied: false,
            capped: false,
            ramped_slowly: false,
            rough: false,
            loaded_ms,
        };
        Report {
            started_at_ms: 1_792_771_200_000,
            duration_ms: 15_000,
            provider: "Cloudflare".into(),
            colo: Some("FRA".into()),
            latency: Some(IdleLatency {
                median_ms: 14.04,
                jitter_ms: Some(2.25),
                answered: 19,
                sent: 19,
            }),
            download: speed(312.44, Some(62.0)),
            upload: Some(speed(41.2, Some(48.0))),
            upload_error: None,
            bytes_used: 238_400_000,
            scheduled: true,
        }
    }

    #[test]
    fn a_result_has_the_csv_fields_and_the_verdict() {
        let offset = FixedOffset::east_opt(5 * 3600).unwrap();
        let value = result(&report(), &offset);
        assert_eq!(value["tested_at"], "2026-10-23T21:00:00+05:00");
        assert_eq!(value["server"], "Cloudflare FRA");
        assert_eq!(value["download_mbps"], 312.4);
        assert_eq!(value["upload_mbps"], 41.2);
        assert_eq!(value["latency_ms"], 14.0);
        assert_eq!(value["latency_under_load_ms"], 62.0);
        assert_eq!(value["data_used_mb"], 238.4);
        assert_eq!(value["scheduled"], true);
        assert_eq!(value["health"], "good");
        assert_eq!(value["verdict"], "Good for 4K video and calls");
        assert_eq!(value["notes"], json!(["scheduled"]));
    }

    #[test]
    fn last_skips_failures_and_history_keeps_everything() {
        let offset = FixedOffset::east_opt(0).unwrap();
        let entries = vec![
            Entry::Result {
                report: Box::new(report()),
            },
            Entry::Failed {
                at_ms: 1_792_771_300_000,
                error: TestError::Stalled,
            },
            Entry::Outage {
                start_ms: 1_792_771_400_000,
                end_ms: 1_792_771_400_000 + 7 * 60_000,
            },
        ];
        assert_eq!(last(&entries, &offset)["download_mbps"], 312.4);
        assert_eq!(last(&[], &offset), Value::Null);
        let all = history(&entries, &offset);
        assert_eq!(all[1]["result"], "failed");
        assert_eq!(all[1]["error"], "stalled");
        assert_eq!(all[1]["message"], "Connection stalled");
        assert_eq!(all[2]["result"], "outage");
        assert_eq!(all[2]["minutes"], 7);
    }
}
