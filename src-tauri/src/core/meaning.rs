//! What a result means for ordinary use. Thresholds: docs/method.md, "Plain meaning".

use super::speedtest::Report;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Health {
    Good,
    Fair,
    Poor,
}

pub fn health(report: &Report) -> Health {
    let down = report.download.mbps;
    let up = report.upload.as_ref().map(|speed| speed.mbps);
    let loaded = report.loaded_ms();
    let jitter = report
        .latency
        .as_ref()
        .and_then(|latency| latency.jitter_ms);
    if down < 5.0 || up.is_some_and(|up| up < 1.5) || loaded.is_some_and(|ms| ms >= 200.0) {
        return Health::Poor;
    }
    let good = down >= 25.0
        && up.is_some_and(|up| up >= 4.0)
        && loaded.is_none_or(|ms| ms < 100.0)
        && jitter.is_none_or(|ms| ms < 30.0);
    if good {
        Health::Good
    } else {
        Health::Fair
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::speedtest::{IdleLatency, Speed};

    fn speed(mbps: f64, loaded_ms: Option<f64>) -> Speed {
        Speed {
            mbps,
            p10: mbps,
            p90: mbps,
            varied: false,
            capped: false,
            ramped_slowly: false,
            rough: false,
            loaded_ms,
        }
    }

    pub fn report(down: f64, up: Option<f64>, loaded: Option<f64>, jitter: f64) -> Report {
        Report {
            started_at_ms: 0,
            duration_ms: 15_000,
            provider: "Cloudflare",
            colo: None,
            latency: Some(IdleLatency {
                median_ms: 14.0,
                jitter_ms: Some(jitter),
                answered: 19,
                sent: 19,
            }),
            download: speed(down, loaded),
            upload: up.map(|up| speed(up, None)),
            upload_error: None,
            bytes_used: 0,
        }
    }

    #[test]
    fn fast_steady_line_is_good() {
        assert_eq!(
            health(&report(312.0, Some(41.0), Some(62.0), 2.0)),
            Health::Good
        );
        assert_eq!(
            health(&report(25.0, Some(4.0), Some(99.0), 29.0)),
            Health::Good
        );
    }

    #[test]
    fn bufferbloat_or_jitter_makes_it_fair() {
        assert_eq!(
            health(&report(300.0, Some(40.0), Some(150.0), 2.0)),
            Health::Fair
        );
        assert_eq!(
            health(&report(300.0, Some(40.0), Some(60.0), 30.0)),
            Health::Fair
        );
        assert_eq!(health(&report(300.0, None, Some(60.0), 2.0)), Health::Fair);
        assert_eq!(
            health(&report(20.0, Some(10.0), Some(60.0), 2.0)),
            Health::Fair
        );
    }

    #[test]
    fn slow_or_laggy_line_is_poor() {
        assert_eq!(
            health(&report(4.9, Some(10.0), Some(60.0), 2.0)),
            Health::Poor
        );
        assert_eq!(
            health(&report(300.0, Some(1.4), Some(60.0), 2.0)),
            Health::Poor
        );
        assert_eq!(
            health(&report(300.0, Some(40.0), Some(200.0), 2.0)),
            Health::Poor
        );
    }
}
