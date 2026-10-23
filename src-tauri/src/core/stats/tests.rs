use super::*;

const DOWNLOAD: Limits = Limits {
    min_ms: 5_000,
    max_ms: 10_000,
    max_bytes: 380_000_000,
};

fn fixture(name: &str) -> Vec<Reading> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/throughput")
        .join(format!("{name}.json"));
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    value["readings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pair| Reading {
            at_ms: pair[0].as_u64().unwrap(),
            bytes: pair[1].as_u64().unwrap(),
        })
        .collect()
}

/// Feed readings the way the runner does and stop at the first decision.
fn replay(name: &str, limits: Limits) -> (Tracker, Decision) {
    let mut tracker = Tracker::new();
    for reading in fixture(name) {
        tracker.push(reading);
        let decision = tracker.decide(limits, 6, 6);
        if decision != Decision::Continue {
            return (tracker, decision);
        }
    }
    (tracker, Decision::Continue)
}

fn close(actual: f64, expected: f64, tolerance: f64) -> bool {
    (actual - expected).abs() <= expected * tolerance
}

#[test]
fn steady_link_converges_on_its_sustained_rate() {
    let (tracker, decision) = replay("steady-300", DOWNLOAD);
    assert_eq!(decision, Decision::Stop(Stop::Converged));
    assert!((5_000..7_000).contains(&tracker.elapsed_ms()));
    let result = tracker.result(Stop::Converged).unwrap();
    assert!(close(result.mbps, 300.0, 0.03), "{}", result.mbps);
    assert!((1_000..=2_500).contains(&result.steady_start_ms));
    assert!(!result.ramped_slowly && !result.capped && !result.varied());
    assert!(result.p10 <= result.p50 && result.p50 <= result.p90);
}

#[test]
fn result_is_never_the_peak() {
    let (tracker, decision) = replay("spiky-100", DOWNLOAD);
    let Decision::Stop(stop) = decision else {
        panic!("{decision:?}")
    };
    let result = tracker.result(stop).unwrap();
    let peak = tracker.bins().into_iter().fold(0.0, f64::max);
    assert!(result.mbps < peak * 0.9, "{} vs peak {peak}", result.mbps);
    assert!(close(result.mbps, 100.0, 0.1), "{}", result.mbps);
    assert!(result.varied());
}

#[test]
fn slow_ramp_starts_the_window_at_four_seconds() {
    let (tracker, decision) = replay("slow-ramp-200", DOWNLOAD);
    let Decision::Stop(stop) = decision else {
        panic!("{decision:?}")
    };
    let result = tracker.result(stop).unwrap();
    assert_eq!(result.steady_start_ms, 4_000);
    assert!(result.ramped_slowly);
    assert!(
        result.mbps > 150.0 && result.mbps < 200.0,
        "{}",
        result.mbps
    );
}

#[test]
fn stalled_link_fails_without_a_number() {
    let (tracker, decision) = replay("stall", DOWNLOAD);
    assert_eq!(decision, Decision::Fail(Failure::Stalled));
    assert!((5_400..5_700).contains(&tracker.elapsed_ms()));
}

#[test]
fn cap_stops_the_test_and_labels_the_result() {
    let (tracker, decision) = replay("cap-gigabit", DOWNLOAD);
    assert_eq!(decision, Decision::Stop(Stop::Cap));
    let result = tracker.result(Stop::Cap).unwrap();
    assert!(result.capped);
    assert!(close(result.mbps, 1_000.0, 0.05), "{}", result.mbps);
    assert!(result.bytes >= DOWNLOAD.max_bytes);
}

#[test]
fn cut_off_test_is_too_short() {
    let tracker = Tracker::from_readings(&fixture("too-short"));
    assert_eq!(tracker.result(Stop::Time), Err(Failure::TooShort));
    assert_eq!(tracker.result(Stop::Cap), Err(Failure::TooShort));
}

#[test]
fn losing_half_the_streams_fails() {
    let tracker = Tracker::from_readings(&fixture("steady-300")[..20]);
    assert_eq!(tracker.decide(DOWNLOAD, 3, 6), Decision::Continue);
    assert_eq!(
        tracker.decide(DOWNLOAD, 2, 6),
        Decision::Fail(Failure::Dropped)
    );
}

#[test]
fn time_limit_stops_a_link_that_never_settles() {
    let limits = Limits {
        max_ms: 6_000,
        ..DOWNLOAD
    };
    let (tracker, decision) = replay("slow-ramp-200", limits);
    assert_eq!(decision, Decision::Stop(Stop::Time));
    assert!(tracker.elapsed_ms() >= 6_000);
}

#[test]
fn tracker_ignores_out_of_order_and_shrinking_readings() {
    let mut tracker = Tracker::new();
    tracker.push(Reading {
        at_ms: 100,
        bytes: 1_000,
    });
    tracker.push(Reading {
        at_ms: 90,
        bytes: 5_000,
    });
    tracker.push(Reading {
        at_ms: 200,
        bytes: 500,
    });
    assert_eq!(tracker.elapsed_ms(), 200);
    assert_eq!(tracker.bytes(), 1_000);
}

#[test]
fn live_rate_uses_the_last_second() {
    let readings: Vec<Reading> = (1..=20)
        .map(|step| Reading {
            at_ms: step * 100,
            bytes: if step <= 10 {
                0
            } else {
                (step - 10) * 1_250_000
            },
        })
        .collect();
    let tracker = Tracker::from_readings(&readings);
    assert!(
        close(tracker.live_mbps(), 100.0, 0.001),
        "{}",
        tracker.live_mbps()
    );
}

#[test]
fn percentiles_interpolate_and_skip_non_finite() {
    let values = [5.0, 1.0, f64::NAN, 3.0, 2.0, 4.0];
    assert_eq!(median(&values), Some(3.0));
    assert_eq!(percentile(&values, 0.1), Some(1.4));
    assert_eq!(percentile(&values, 0.9), Some(4.6));
    assert_eq!(median(&[]), None);
}

#[test]
fn jitter_is_mean_absolute_change() {
    assert_eq!(jitter(&[10.0, 14.0, 12.0, 12.0]), Some(2.0));
    assert_eq!(jitter(&[10.0]), None);
}

#[test]
fn server_timing_prefers_request_duration_then_sums_speed_metrics() {
    assert_eq!(
        server_time_ms("cfSpeedEdge;dur=3, cfSpeedWorker;dur=17"),
        Some(20.0)
    );
    assert_eq!(
        server_time_ms("cfSpeedEdge;dur=3, cfReqDur;dur=12.5"),
        Some(12.5)
    );
    assert_eq!(
        server_time_ms("cfL4;desc=\"?proto=TCP&rtt=1234\", cdn-cache; desc=HIT"),
        None
    );
    assert_eq!(server_time_ms("cfSpeedEdge;dur=abc"), None);
}

#[test]
fn probe_rtt_subtracts_server_time_only_when_sensible() {
    assert_eq!(probe_rtt(45.0, Some(20.0)), 25.0);
    assert_eq!(probe_rtt(20.5, Some(20.0)), 20.5);
    assert_eq!(probe_rtt(30.0, None), 30.0);
}

#[test]
fn idle_latency_drops_the_warm_up_probe_and_needs_ten_answers() {
    let mut probes = vec![Some(90.0)];
    probes.extend([12.0, 14.0, 13.0, 15.0, 12.0, 30.0, 13.0, 14.0, 12.0, 13.0].map(Some));
    probes.extend([None; 9]);
    let latency = idle_latency(&probes).unwrap();
    assert_eq!(latency.median_ms, 13.0);
    assert_eq!(latency.answered, 10);
    assert_eq!(latency.sent, 19);
    assert!(latency.jitter_ms.unwrap() > 4.0);
    probes[3] = None;
    assert_eq!(idle_latency(&probes), None);
}

#[test]
fn loaded_latency_counts_timeouts_inside_the_window_only() {
    let probe = |sent_ms, rtt_ms| Probe { sent_ms, rtt_ms };
    let probes = [
        probe(500, Some(15.0)),
        probe(2_000, Some(60.0)),
        probe(2_250, None),
        probe(2_500, Some(70.0)),
        probe(2_750, None),
        probe(3_000, None),
        probe(9_000, Some(10.0)),
    ];
    assert_eq!(
        loaded_latency(&probes, 1_000, 8_000),
        Some(PROBE_TIMEOUT_MS)
    );
    assert_eq!(loaded_latency(&probes[..4], 1_000, 8_000), None);
}

#[test]
fn upload_uses_completed_requests_per_stream() {
    let request = |stream, started_ms, bytes, duration_ms| Completed {
        stream,
        started_ms,
        bytes,
        duration_ms,
    };
    let mut requests = Vec::new();
    for stream in 0..2 {
        requests.push(request(stream, 200, 50_000, 400.0));
        for step in 0..3 {
            requests.push(request(stream, 1_000 + step * 1_000, 2_500_000, 1_000.0));
        }
    }
    let rate = upload_mbps(&requests, 2, 1_000).unwrap();
    assert!(close(rate, 40.0, 0.001), "{rate}");
    assert_eq!(upload_mbps(&requests, 3, 1_000), None);
    assert_eq!(upload_mbps(&requests, 2, 2_500), None);
}

#[test]
fn data_estimate_follows_recent_results_and_rounds_up() {
    assert_eq!(data_estimate(&[]), DataEstimate::UpTo(600));
    assert_eq!(data_estimate(&[(100.0, 20.0)]), DataEstimate::About(110));
    assert_eq!(data_estimate(&[(300.0, 40.0)]), DataEstimate::About(310));
    assert_eq!(data_estimate(&[(1_000.0, 100.0)]), DataEstimate::About(480));
    assert_eq!(data_estimate(&[(10.0, 2.0)]), DataEstimate::About(15));
    assert_eq!(
        data_estimate(&[(900.0, 90.0), (100.0, 20.0), (110.0, 22.0), (90.0, 18.0)]),
        DataEstimate::About(110)
    );
}
