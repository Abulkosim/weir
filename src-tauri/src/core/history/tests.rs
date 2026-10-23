use super::*;
use crate::core::speedtest::{IdleLatency, Speed};

const DAY: u64 = 24 * 60 * 60 * 1000;
const NOW: u64 = 400 * DAY;

fn report(at_ms: u64, down: f64) -> Report {
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
        started_at_ms: at_ms,
        duration_ms: 15_000,
        provider: "Cloudflare".into(),
        colo: Some("FRA".into()),
        latency: Some(IdleLatency {
            median_ms: 12.0,
            jitter_ms: Some(1.5),
            answered: 19,
            sent: 19,
        }),
        download: speed(down),
        upload: Some(speed(down / 10.0)),
        upload_error: None,
        bytes_used: 200_000_000,
    }
}

fn result(at_ms: u64, down: f64) -> Entry {
    Entry::Result {
        report: Box::new(report(at_ms, down)),
    }
}

#[test]
fn saves_and_loads_every_entry_in_order() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("history.json");
    let (mut history, recovery) = History::load(path.clone(), NOW);
    assert!(history.is_empty() && recovery.is_none());
    history.push(result(NOW - 2 * DAY, 300.0), NOW).unwrap();
    history
        .push(
            Entry::Failed {
                at_ms: NOW - DAY,
                error: TestError::Stalled,
            },
            NOW,
        )
        .unwrap();
    history.push(result(NOW - 3 * DAY, 280.0), NOW).unwrap();
    let (loaded, recovery) = History::load(path.clone(), NOW);
    assert_eq!(recovery, None);
    assert_eq!(loaded.entries(), history.entries());
    assert_eq!(loaded.entries()[0].at_ms(), NOW - 3 * DAY);
    assert_eq!(loaded.last_report().unwrap().download.mbps, 300.0);
    assert_eq!(loaded.last_failure(), Some((NOW - DAY, TestError::Stalled)));
    assert!(!path.with_extension("json.tmp").exists());
}

#[test]
fn keeps_one_year_and_a_bounded_count() {
    let directory = tempfile::tempdir().unwrap();
    let (mut history, _) = History::load(directory.path().join("history.json"), NOW);
    history
        .push(result(NOW - RETENTION_MS - 1, 10.0), NOW)
        .unwrap();
    history
        .push(result(NOW - RETENTION_MS + DAY, 20.0), NOW)
        .unwrap();
    assert_eq!(history.len(), 1);
    for index in 0..MAX_ENTRIES as u64 + 10 {
        history.entries.push(result(NOW - DAY + index, 50.0));
    }
    history.push(result(NOW, 99.0), NOW).unwrap();
    assert_eq!(history.len(), MAX_ENTRIES);
    assert_eq!(history.last_report().unwrap().download.mbps, 99.0);
}

#[test]
fn unreadable_entries_are_skipped_and_counted() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("history.json");
    let good = serde_json::to_value(result(NOW - DAY, 120.0)).unwrap();
    let text = serde_json::json!({
        "version": 1,
        "entries": [good, {"kind": "result", "report": {"broken": true}}, {"kind": "mystery"}]
    });
    std::fs::write(&path, text.to_string()).unwrap();
    let (history, recovery) = History::load(path, NOW);
    assert_eq!(history.len(), 1);
    assert_eq!(recovery, Some(Recovery::Skipped(2)));
}

#[test]
fn corrupt_file_is_moved_aside_not_overwritten() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("history.json");
    std::fs::write(&path, "{ not json").unwrap();
    let (history, recovery) = History::load(path.clone(), NOW);
    assert!(history.is_empty());
    let Some(Recovery::BackedUp(backup)) = recovery else {
        panic!("{recovery:?}")
    };
    assert_eq!(std::fs::read_to_string(backup).unwrap(), "{ not json");
    assert!(!path.exists());
}

#[test]
fn recent_and_speeds_serve_the_popover_and_estimate() {
    let directory = tempfile::tempdir().unwrap();
    let (mut history, _) = History::load(directory.path().join("history.json"), NOW);
    for (day, down) in [(5, 100.0), (4, 110.0), (3, 90.0), (2, 120.0)] {
        history.push(result(NOW - day * DAY, down), NOW).unwrap();
    }
    history
        .push(
            Entry::Failed {
                at_ms: NOW - DAY,
                error: TestError::Offline,
            },
            NOW,
        )
        .unwrap();
    let recent = history.recent(2);
    assert!(matches!(recent[0], Entry::Failed { .. }));
    assert_eq!(recent[1].report().unwrap().download.mbps, 120.0);
    assert_eq!(
        history.speeds(3),
        vec![(110.0, 11.0), (90.0, 9.0), (120.0, 12.0)]
    );
}

#[test]
fn stored_entries_hold_no_address_or_location() {
    let text = serde_json::to_string(&result(NOW, 300.0)).unwrap();
    for field in ["ip", "city", "asn", "latitude", "longitude", "ssid"] {
        assert!(
            !text.to_lowercase().contains(&format!("\"{field}\"")),
            "{field}"
        );
    }
}
