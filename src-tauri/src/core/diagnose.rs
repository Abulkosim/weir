//! "Is it me or the internet?": four readings in, one plain sentence out.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Good,
    Skipped,
    Fair,
    Bad,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Wifi {
    pub rssi_dbm: Option<i32>,
    pub noise_dbm: Option<i32>,
    pub rate_mbps: Option<f64>,
    /// Windows signal bars, 0–5, when RSSI is not available.
    pub bars: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Router {
    pub sent: u32,
    pub answered: u32,
    pub median_ms: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Readings {
    pub online: bool,
    pub wired: bool,
    /// `None` when not on Wi-Fi.
    pub wifi: Option<Wifi>,
    /// `None` when the router address is unknown.
    pub router: Option<Router>,
    /// `None` when there is no DNS server to ask; `Some(None)` when lookups failed.
    pub dns_ms: Option<Option<f64>>,
    /// `None` when not checked; `Some(None)` when unreachable.
    pub internet_ms: Option<Option<f64>>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub key: &'static str,
    pub label: &'static str,
    pub status: Status,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnosis {
    pub status: Status,
    pub headline: String,
    pub steps: Vec<Step>,
    pub checked_at_ms: u64,
}

pub const WIFI_GOOD_DBM: i32 = -64;
pub const WIFI_WEAK_DBM: i32 = -75;
pub const ROUTER_SLOW_MS: f64 = 50.0;
pub const DNS_SLOW_MS: f64 = 300.0;

fn ms(value: f64) -> String {
    if value < 1.0 {
        "<1 ms".into()
    } else {
        format!("{value:.0} ms")
    }
}

fn wifi_step(wifi: &Wifi) -> Step {
    let (status, text) = match (wifi.rssi_dbm, wifi.bars) {
        (Some(rssi), _) => {
            let status = if rssi >= WIFI_GOOD_DBM {
                Status::Good
            } else if rssi > WIFI_WEAK_DBM {
                Status::Fair
            } else {
                Status::Bad
            };
            let word = match status {
                Status::Good => "Strong",
                Status::Fair => "Fair",
                _ => "Weak",
            };
            let rate = wifi
                .rate_mbps
                .filter(|rate| *rate > 0.0)
                .map(|rate| format!(" · link {rate:.0} Mbps"))
                .unwrap_or_default();
            (status, format!("{word} signal · {rssi} dBm{rate}"))
        }
        (None, Some(bars)) => {
            let status = match bars {
                4.. => Status::Good,
                3 => Status::Fair,
                _ => Status::Bad,
            };
            (status, format!("Signal {bars} of 5 bars"))
        }
        (None, None) => (Status::Skipped, "Signal not available".into()),
    };
    Step {
        key: "wifi",
        label: "Wi-Fi",
        status,
        text,
    }
}

fn router_step(router: Option<&Router>) -> Step {
    let (status, text) = match router {
        None => (Status::Skipped, "Router address not found".into()),
        Some(router) if router.answered == 0 => (Status::Bad, "Not answering".into()),
        Some(router) if router.sent >= 2 && router.answered * 5 <= router.sent * 3 => (
            Status::Fair,
            format!(
                "Dropped {} of {} replies",
                router.sent - router.answered,
                router.sent
            ),
        ),
        Some(Router {
            median_ms: Some(median),
            ..
        }) if *median >= ROUTER_SLOW_MS => {
            (Status::Fair, format!("Slow to answer · {}", ms(*median)))
        }
        Some(router) => (
            Status::Good,
            router.median_ms.map_or_else(
                || "Answering".into(),
                |median| format!("Answers in {}", ms(median)),
            ),
        ),
    };
    Step {
        key: "router",
        label: "Router",
        status,
        text,
    }
}

fn dns_step(dns_ms: Option<Option<f64>>) -> Step {
    let (status, text) = match dns_ms {
        None => (Status::Skipped, "No DNS server found".into()),
        Some(None) => (Status::Bad, "Lookups fail".into()),
        Some(Some(value)) if value >= DNS_SLOW_MS => {
            (Status::Fair, format!("Slow lookups · {}", ms(value)))
        }
        Some(Some(value)) => (Status::Good, format!("Lookups in {}", ms(value))),
    };
    Step {
        key: "dns",
        label: "DNS",
        status,
        text,
    }
}

fn internet_step(internet_ms: Option<Option<f64>>) -> Step {
    let (status, text) = match internet_ms {
        None => (Status::Skipped, "Not checked".into()),
        Some(None) => (Status::Bad, "Cloudflare not reachable".into()),
        Some(Some(value)) => (Status::Good, format!("Cloudflare in {}", ms(value))),
    };
    Step {
        key: "internet",
        label: "Internet",
        status,
        text,
    }
}

fn headline(readings: &Readings, steps: &[Step]) -> String {
    let status = |key: &str| {
        steps
            .iter()
            .find(|step| step.key == key)
            .map_or(Status::Skipped, |step| step.status)
    };
    let rssi = readings
        .wifi
        .and_then(|wifi| wifi.rssi_dbm)
        .map(|rssi| format!(" ({rssi} dBm)"))
        .unwrap_or_default();
    let local = if readings.wifi.is_some() {
        "Wi-Fi"
    } else {
        "network"
    };
    if !readings.online {
        return "No network connection. Check Wi-Fi or the cable.".into();
    }
    if status("router") == Status::Bad {
        return if status("wifi") == Status::Bad {
            format!("Your Wi-Fi signal is weak{rssi} and the router isn’t answering. Move closer to the router.")
        } else {
            "Your router isn’t answering. Restart it, or check the cable.".into()
        };
    }
    if status("internet") == Status::Bad {
        return format!("Your {local} is fine. The problem is past your router.");
    }
    if status("dns") == Status::Bad {
        return "Websites can’t be looked up. Your DNS server isn’t answering.".into();
    }
    if status("dns") == Status::Fair {
        let value = readings
            .dns_ms
            .flatten()
            .map(|value| format!(": looking up names takes {}", ms(value)))
            .unwrap_or_default();
        return format!("Websites are slow to open{value}.");
    }
    if status("wifi") == Status::Bad {
        return format!("Your Wi-Fi signal is weak{rssi}. Move closer to the router.");
    }
    if status("router") == Status::Fair {
        return "Your router is slow to answer or drops replies.".into();
    }
    if status("wifi") == Status::Fair {
        return format!("Wi-Fi signal is fair{rssi}. Fine for most things.");
    }
    if readings.wired && readings.wifi.is_none() {
        return "Everything looks fine from here, on a wired connection.".into();
    }
    "Everything looks fine from here.".into()
}

pub fn diagnose(readings: &Readings, checked_at_ms: u64) -> Diagnosis {
    let mut steps = Vec::with_capacity(4);
    if let Some(wifi) = &readings.wifi {
        steps.push(wifi_step(wifi));
    }
    steps.push(router_step(readings.router.as_ref()));
    steps.push(dns_step(readings.dns_ms));
    steps.push(internet_step(readings.internet_ms));
    if !readings.online {
        for step in &mut steps {
            if step.status == Status::Good {
                step.status = Status::Skipped;
            }
        }
    }
    let status = if readings.online {
        steps
            .iter()
            .map(|step| step.status)
            .max()
            .unwrap_or(Status::Good)
    } else {
        Status::Bad
    };
    Diagnosis {
        status,
        headline: headline(readings, &steps),
        steps,
        checked_at_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy() -> Readings {
        Readings {
            online: true,
            wired: false,
            wifi: Some(Wifi {
                rssi_dbm: Some(-52),
                noise_dbm: Some(-92),
                rate_mbps: Some(866.0),
                bars: None,
            }),
            router: Some(Router {
                sent: 5,
                answered: 5,
                median_ms: Some(3.2),
            }),
            dns_ms: Some(Some(18.0)),
            internet_ms: Some(Some(24.0)),
        }
    }

    #[test]
    fn healthy_connection_says_so_with_each_step() {
        let diagnosis = diagnose(&healthy(), 1);
        assert_eq!(diagnosis.status, Status::Good);
        assert_eq!(diagnosis.headline, "Everything looks fine from here.");
        let texts: Vec<&str> = diagnosis
            .steps
            .iter()
            .map(|step| step.text.as_str())
            .collect();
        assert_eq!(
            texts,
            [
                "Strong signal · -52 dBm · link 866 Mbps",
                "Answers in 3 ms",
                "Lookups in 18 ms",
                "Cloudflare in 24 ms"
            ]
        );
    }

    #[test]
    fn local_fine_but_internet_down_points_past_the_router() {
        let readings = Readings {
            internet_ms: Some(None),
            dns_ms: Some(None),
            ..healthy()
        };
        let diagnosis = diagnose(&readings, 1);
        assert_eq!(
            diagnosis.headline,
            "Your Wi-Fi is fine. The problem is past your router."
        );
        assert_eq!(diagnosis.status, Status::Bad);
        let wired = Readings {
            wifi: None,
            wired: true,
            ..readings
        };
        assert_eq!(
            diagnose(&wired, 1).headline,
            "Your network is fine. The problem is past your router."
        );
    }

    #[test]
    fn silent_router_is_named_and_weak_wifi_explains_it() {
        let mut readings = Readings {
            router: Some(Router {
                sent: 5,
                answered: 0,
                median_ms: None,
            }),
            internet_ms: Some(None),
            ..healthy()
        };
        assert_eq!(
            diagnose(&readings, 1).headline,
            "Your router isn’t answering. Restart it, or check the cable."
        );
        readings.wifi = Some(Wifi {
            rssi_dbm: Some(-81),
            ..Wifi::default()
        });
        assert_eq!(
            diagnose(&readings, 1).headline,
            "Your Wi-Fi signal is weak (-81 dBm) and the router isn’t answering. Move closer to the router."
        );
    }

    #[test]
    fn dns_problems_are_told_apart_from_outages() {
        let failing = Readings {
            dns_ms: Some(None),
            ..healthy()
        };
        assert_eq!(
            diagnose(&failing, 1).headline,
            "Websites can’t be looked up. Your DNS server isn’t answering."
        );
        let slow = Readings {
            dns_ms: Some(Some(450.0)),
            ..healthy()
        };
        assert_eq!(
            diagnose(&slow, 1).headline,
            "Websites are slow to open: looking up names takes 450 ms."
        );
    }

    #[test]
    fn wifi_bands_and_router_quality() {
        let wifi = |rssi| {
            wifi_step(&Wifi {
                rssi_dbm: Some(rssi),
                ..Wifi::default()
            })
        };
        assert_eq!(wifi(-64).status, Status::Good);
        assert_eq!(wifi(-65).status, Status::Fair);
        assert_eq!(wifi(-75).status, Status::Bad);
        let bars = |bars| {
            wifi_step(&Wifi {
                bars: Some(bars),
                ..Wifi::default()
            })
        };
        assert_eq!(bars(4).status, Status::Good);
        assert_eq!(bars(2).text, "Signal 2 of 5 bars");
        let router = |answered, median| {
            router_step(Some(&Router {
                sent: 5,
                answered,
                median_ms: median,
            }))
        };
        assert_eq!(router(2, Some(4.0)).text, "Dropped 3 of 5 replies");
        assert_eq!(router(4, Some(80.0)).text, "Slow to answer · 80 ms");
        assert_eq!(router(5, Some(0.4)).text, "Answers in <1 ms");
        let weak = Readings {
            wifi: Some(Wifi {
                rssi_dbm: Some(-79),
                ..Wifi::default()
            }),
            ..healthy()
        };
        assert_eq!(
            diagnose(&weak, 1).headline,
            "Your Wi-Fi signal is weak (-79 dBm). Move closer to the router."
        );
    }

    #[test]
    fn offline_skips_the_steps_and_says_so() {
        let readings = Readings {
            online: false,
            ..healthy()
        };
        let diagnosis = diagnose(&readings, 1);
        assert_eq!(
            diagnosis.headline,
            "No network connection. Check Wi-Fi or the cable."
        );
        assert_eq!(diagnosis.status, Status::Bad);
        assert!(diagnosis
            .steps
            .iter()
            .all(|step| step.status != Status::Good));
    }

    #[test]
    fn missing_pieces_are_skipped_not_failed() {
        let readings = Readings {
            router: None,
            dns_ms: None,
            ..healthy()
        };
        let diagnosis = diagnose(&readings, 1);
        assert_eq!(diagnosis.headline, "Everything looks fine from here.");
        assert_eq!(diagnosis.status, Status::Skipped);
    }
}
