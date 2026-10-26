//! How a result compares with the speed the provider sells. Bands: docs/method.md, "Your plan".

use super::{meaning::Health, speedtest::Report};
use serde::Serialize;

/// Highest plan speed accepted in Settings, in Mbps.
pub const MAX_MBPS: u32 = 100_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub download_mbps: Option<u32>,
    pub upload_mbps: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Share {
    pub paid_mbps: u32,
    pub percent: u32,
    /// The test stopped at its data limit, so the real share may be higher.
    pub at_least: bool,
    pub health: Health,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub download: Option<Share>,
    pub upload: Option<Share>,
}

/// A plan speed from Settings, or `None` when it is empty or out of range.
pub fn valid(mbps: Option<u32>) -> Option<u32> {
    mbps.filter(|mbps| (1..=MAX_MBPS).contains(mbps))
}

pub fn share(measured_mbps: f64, paid_mbps: u32, at_least: bool) -> Share {
    let percent = (measured_mbps.max(0.0) / f64::from(paid_mbps) * 100.0).round() as u32;
    let health = match percent {
        80.. => Health::Good,
        _ if at_least => Health::Fair,
        50.. => Health::Fair,
        _ => Health::Poor,
    };
    Share {
        paid_mbps,
        percent,
        at_least,
        health,
    }
}

pub fn compare(report: &Report, plan: Plan) -> Option<Comparison> {
    let download = valid(plan.download_mbps)
        .map(|paid| share(report.download.mbps, paid, report.download.capped));
    let upload = valid(plan.upload_mbps).and_then(|paid| {
        report
            .upload
            .as_ref()
            .map(|speed| share(speed.mbps, paid, speed.capped))
    });
    (download.is_some() || upload.is_some()).then_some(Comparison { download, upload })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::speedtest::Speed;

    fn speed(mbps: f64, capped: bool) -> Speed {
        Speed {
            mbps,
            p10: mbps,
            p90: mbps,
            varied: false,
            capped,
            ramped_slowly: false,
            rough: false,
            loaded_ms: None,
        }
    }

    fn report(down: Speed, up: Option<Speed>) -> Report {
        Report {
            started_at_ms: 0,
            duration_ms: 15_000,
            provider: "Cloudflare".into(),
            colo: None,
            latency: None,
            download: down,
            upload: up,
            upload_error: None,
            bytes_used: 0,
        }
    }

    #[test]
    fn bands_split_at_80_and_50_percent() {
        assert_eq!(share(400.0, 500, false).health, Health::Good);
        assert_eq!(share(395.0, 500, false).health, Health::Fair);
        assert_eq!(share(250.0, 500, false).health, Health::Fair);
        assert_eq!(share(245.0, 500, false).health, Health::Poor);
        assert_eq!(share(540.0, 500, false).percent, 108);
    }

    #[test]
    fn a_capped_test_is_never_called_poor() {
        let capped = share(940.0, 2_000, true);
        assert_eq!((capped.percent, capped.at_least), (47, true));
        assert_eq!(capped.health, Health::Fair);
    }

    #[test]
    fn compares_only_what_was_set_and_measured() {
        let plan = Plan {
            download_mbps: Some(500),
            upload_mbps: Some(50),
        };
        let full = compare(&report(speed(312.4, false), Some(speed(41.2, false))), plan).unwrap();
        assert_eq!(full.download.unwrap().percent, 62);
        assert_eq!(full.upload.unwrap().percent, 82);
        let no_upload = compare(&report(speed(312.4, false), None), plan).unwrap();
        assert_eq!(no_upload.upload, None);
        assert_eq!(
            compare(&report(speed(312.4, false), None), Plan::default()),
            None
        );
        let zero = Plan {
            download_mbps: Some(0),
            upload_mbps: None,
        };
        assert_eq!(compare(&report(speed(312.4, false), None), zero), None);
    }
}
