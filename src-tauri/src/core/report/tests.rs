use super::*;
use crate::core::speedtest::{IdleLatency, Speed};

const HOUR: u64 = 3_600_000;
const NOW: u64 = 1_792_771_200_000;

fn report(at_ms: u64, down: f64) -> Report {
    let speed = |mbps| Speed {
        mbps,
        p10: mbps,
        p90: mbps,
        varied: false,
        capped: false,
        ramped_slowly: false,
        rough: false,
        loaded_ms: Some(48.0),
    };
    Report {
        started_at_ms: at_ms,
        duration_ms: 15_000,
        provider: "Cloudflare".into(),
        colo: Some("FRA".into()),
        latency: Some(IdleLatency {
            median_ms: 14.0,
            jitter_ms: Some(2.0),
            answered: 19,
            sent: 19,
        }),
        download: speed(down),
        upload: Some(speed(down / 8.0)),
        upload_error: None,
        bytes_used: 200_000_000,
    }
}

fn result(at_ms: u64, down: f64) -> Entry {
    Entry::Result {
        report: Box::new(report(at_ms, down)),
    }
}

fn options(plan: Plan) -> Options<'static> {
    Options {
        now_ms: NOW,
        offset: FixedOffset::east_opt(0).unwrap(),
        plan,
        watching: true,
        version: "0.4.0",
    }
}

const PLAN: Plan = Plan {
    download_mbps: Some(500),
    upload_mbps: Some(50),
};

/// Two weeks of tests at 09:00, 13:00 and 21:00, slow in the evening, with one outage and one failure.
fn month() -> Vec<Entry> {
    let mut entries = Vec::new();
    for day in 0..14 {
        let base = NOW - (day + 1) * 24 * HOUR;
        let base = base - base % (24 * HOUR);
        entries.push(result(base + 9 * HOUR, 310.0));
        entries.push(result(base + 13 * HOUR, 300.0));
        entries.push(result(base + 21 * HOUR, 140.0));
    }
    let day = NOW - NOW % (24 * HOUR) - 3 * 24 * HOUR;
    entries.push(Entry::Outage {
        start_ms: day + 14 * HOUR + 2 * 60_000,
        end_ms: day + 14 * HOUR + 31 * 60_000,
    });
    entries.push(Entry::Failed {
        at_ms: day + 14 * HOUR + 5 * 60_000,
        error: TestError::Stalled,
    });
    entries.push(result(NOW - 40 * 24 * HOUR, 10.0));
    entries.sort_by_key(Entry::at_ms);
    entries
}

#[test]
fn summary_covers_only_the_last_30_days() {
    let entries = month();
    let summary = summarize(&entries, &options(PLAN)).unwrap();
    assert_eq!(summary.tests, 42, "the 40-day-old test is left out");
    assert_eq!(summary.failed.len(), 1);
    assert_eq!(summary.outages.len(), 1);
    assert_eq!(summary.download.unwrap().median, 300.0);
    assert_eq!(summary.download.unwrap().low, 140.0);
    assert_eq!(summary.under_half, Some(14));
    assert!(summary
        .entries
        .windows(2)
        .all(|pair| pair[0].at_ms() >= pair[1].at_ms()));
    assert_eq!(summarize(&entries[..1], &options(PLAN)), None);
}

#[test]
fn slow_evening_hours_are_named() {
    let entries = month();
    let summary = summarize(&entries, &options(PLAN)).unwrap();
    let slow = summary.slow.unwrap();
    assert_eq!(
        (slow.from, slow.to, slow.median, slow.usual),
        (21, 22, 140.0, 300.0)
    );
    let findings = findings(&summary, &options(PLAN));
    assert_eq!(
        findings,
        [
            "Download was under half of the 500 Mbps plan in 14 of 42 tests.",
            "Slowest around 21:00–22:00: 140 Mbps median, against 300 Mbps over the whole period.",
            "No internet once, 29 min in all. Longest: 20 Oct, 14:02–14:31 (29 min).",
            "1 test failed: Connection stalled (20 Oct, 14:05).",
        ]
    );
}

#[test]
fn findings_say_what_was_not_watched_or_set() {
    let entries: Vec<Entry> = (1..=3)
        .map(|day| result(NOW - day * 24 * HOUR, 300.0))
        .collect();
    let mut unwatched = options(Plan::default());
    unwatched.watching = false;
    let summary = summarize(&entries, &unwatched).unwrap();
    assert_eq!(
        findings(&summary, &unwatched),
        ["Outages were not monitored, so drops between tests are not included."]
    );
}

#[test]
fn pdf_is_a_paginated_document_with_plain_text() {
    let entries = month();
    let bytes = pdf(&entries, &options(PLAN)).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(bytes.ends_with(b"%%EOF") || bytes.ends_with(b"%%EOF\n"));
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("(Internet connection report)"));
    assert!(text.contains("/BaseFont /Helvetica-Bold"));
    assert!(
        text.contains("(Page 2 of 2)"),
        "45 rows spill onto a second page"
    );
    assert!(!text.contains("(Page 3 of"));
    assert!(text.contains("(Plan: 500 Mbps down, 50 Mbps up, as entered in Weir)"));
    assert!(pdf(&[], &options(PLAN)).is_none());
}

#[test]
fn text_is_winansi_and_fits() {
    assert_eq!(
        canvas::encode("14:02–14:31 · it’s ≥"),
        b"14:02\x9614:31 \xb7 it\x92s ?"
    );
    assert_eq!(canvas::width("0000", Font::Regular, 10.0), 22.24);
    let cut = canvas::fit("Cloudflare Frankfurt am Main", Font::Regular, 9.0, 60.0);
    assert!(cut.ends_with('…') && canvas::width(&cut, Font::Regular, 9.0) <= 60.0);
    assert_eq!(
        canvas::wrap("one two three four", Font::Regular, 10.0, 40.0),
        ["one two", "three", "four"]
    );
}

#[test]
fn chart_tops_are_round_numbers() {
    assert_eq!(nice_top(312.0), 400.0);
    assert_eq!(nice_top(500.0), 600.0);
    assert_eq!(nice_top(90.0), 100.0);
    assert_eq!(nice_top(20.0), 25.0);
    assert_eq!(nice_top(0.0), 1.0);
}

/// Writes a sample report for a visual check:
/// `WEIR_REPORT_SAMPLE=/tmp/report.pdf cargo test sample_report -- --ignored`.
#[test]
#[ignore]
fn sample_report() {
    let path = std::env::var("WEIR_REPORT_SAMPLE").expect("WEIR_REPORT_SAMPLE");
    let mut options = options(PLAN);
    options.offset = FixedOffset::east_opt(5 * 3600).unwrap();
    std::fs::write(path, pdf(&month(), &options).unwrap()).unwrap();
}
