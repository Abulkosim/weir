//! History as CSV for a spreadsheet or an internet provider. Columns: README "Export".

use super::{
    history::Entry,
    speedtest::{Report, TestError},
};
use chrono::{DateTime, FixedOffset, Local, SecondsFormat};

pub const HEADER: &str = "tested_at,result,server,download_mbps,upload_mbps,latency_ms,latency_under_load_ms,jitter_ms,download_low_mbps,download_high_mbps,data_used_mb,notes";

fn timestamp(ms: u64, offset: &FixedOffset) -> String {
    DateTime::from_timestamp_millis(ms as i64)
        .map(|time| {
            time.with_timezone(offset)
                .to_rfc3339_opts(SecondsFormat::Secs, false)
        })
        .unwrap_or_default()
}

fn number(value: Option<f64>) -> String {
    value.map_or_else(String::new, |value| format!("{value:.1}"))
}

fn field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

pub fn error_text(error: TestError) -> &'static str {
    match error {
        TestError::Offline => "No internet",
        TestError::Refused => "Cloudflare refused the test",
        TestError::ServerError => "Cloudflare server error",
        TestError::Stalled => "Connection stalled",
        TestError::TooShort => "Test too short to measure",
        TestError::Dropped => "Connection kept dropping",
        TestError::NetworkChanged => "Network changed during the test",
        TestError::Cancelled => "Cancelled",
    }
}

fn notes(report: &Report) -> String {
    let mut notes = Vec::new();
    if report.download.capped {
        notes.push("stopped at data limit".to_owned());
    }
    if report.download.varied {
        notes.push("download varied".to_owned());
    }
    if report.download.ramped_slowly {
        notes.push("slow ramp".to_owned());
    }
    match (&report.upload, report.upload_error) {
        (Some(upload), _) if upload.rough => notes.push("upload rough".to_owned()),
        (None, Some(error)) => notes.push(format!("upload failed: {}", error_text(error))),
        _ => {}
    }
    if report.latency.is_none() {
        notes.push("latency not measured".to_owned());
    }
    notes.join("; ")
}

fn row(entry: &Entry, offset: &FixedOffset) -> String {
    let fields: Vec<String> = match entry {
        Entry::Result { report } => vec![
            timestamp(report.started_at_ms, offset),
            "ok".into(),
            field(
                format!(
                    "{} {}",
                    report.provider,
                    report.colo.as_deref().unwrap_or("")
                )
                .trim_end(),
            ),
            number(Some(report.download.mbps)),
            number(report.upload.as_ref().map(|speed| speed.mbps)),
            number(report.latency.as_ref().map(|latency| latency.median_ms)),
            number(report.loaded_ms()),
            number(
                report
                    .latency
                    .as_ref()
                    .and_then(|latency| latency.jitter_ms),
            ),
            number(Some(report.download.p10)),
            number(Some(report.download.p90)),
            number(Some(report.bytes_used as f64 / 1_000_000.0)),
            field(&notes(report)),
        ],
        Entry::Failed { at_ms, error } => {
            let mut fields = vec![timestamp(*at_ms, offset), "failed".into()];
            fields.extend(std::iter::repeat_n(String::new(), 9));
            fields.push(field(error_text(*error)));
            fields
        }
    };
    fields.join(",")
}

/// Oldest first, times in the given offset with the offset written out.
pub fn csv_at(entries: &[Entry], offset: &FixedOffset) -> String {
    let mut text = String::from(HEADER);
    text.push('\n');
    for entry in entries {
        text.push_str(&row(entry, offset));
        text.push('\n');
    }
    text
}

pub fn csv(entries: &[Entry]) -> String {
    csv_at(entries, Local::now().offset())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::speedtest::{IdleLatency, Speed};

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
                median_ms: 14.0,
                jitter_ms: Some(2.25),
                answered: 19,
                sent: 19,
            }),
            download: speed(312.44, Some(62.0)),
            upload: Some(speed(41.2, Some(48.0))),
            upload_error: None,
            bytes_used: 238_400_000,
        }
    }

    #[test]
    fn rows_match_the_header_with_the_offset_written_out() {
        let mut partial = report();
        partial.started_at_ms += 3_600_000;
        partial.upload = None;
        partial.upload_error = Some(TestError::Stalled);
        partial.download.capped = true;
        let entries = [
            Entry::Result {
                report: Box::new(report()),
            },
            Entry::Result {
                report: Box::new(partial),
            },
            Entry::Failed {
                at_ms: 1_792_778_400_000,
                error: TestError::NetworkChanged,
            },
        ];
        let offset = FixedOffset::east_opt(5 * 3600).unwrap();
        let text = csv_at(&entries, &offset);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], HEADER);
        assert_eq!(
            lines[1],
            "2026-10-23T21:00:00+05:00,ok,Cloudflare FRA,312.4,41.2,14.0,62.0,2.2,296.8,328.1,238.4,"
        );
        assert_eq!(
            lines[2],
            "2026-10-23T22:00:00+05:00,ok,Cloudflare FRA,312.4,,14.0,62.0,2.2,296.8,328.1,238.4,stopped at data limit; upload failed: Connection stalled"
        );
        assert_eq!(
            lines[3],
            "2026-10-23T23:00:00+05:00,failed,,,,,,,,,,Network changed during the test"
        );
        let columns = HEADER.split(',').count();
        assert!(lines.iter().all(|line| line.split(',').count() == columns));
    }

    #[test]
    fn fields_with_commas_or_quotes_are_quoted() {
        assert_eq!(field("a, b"), "\"a, b\"");
        assert_eq!(field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(field("plain"), "plain");
    }
}
