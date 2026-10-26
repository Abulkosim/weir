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

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub topic: &'static str,
    pub text: String,
    pub health: Health,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Meaning {
    pub health: Health,
    pub headline: String,
    pub lines: Vec<Line>,
}

fn line(topic: &'static str, health: Health, text: impl Into<String>) -> Line {
    Line {
        topic,
        text: text.into(),
        health,
    }
}

fn video(down: f64) -> Line {
    match down {
        d if d >= 25.0 => line("Video", Health::Good, "Good for 4K video"),
        d if d >= 15.0 => line("Video", Health::Good, "4K on one screen"),
        d if d >= 5.0 => line("Video", Health::Good, "Good for HD video"),
        d if d >= 3.0 => line("Video", Health::Fair, "720p video"),
        _ => line("Video", Health::Poor, "Video may buffer"),
    }
}

fn calls(down: f64, up: Option<f64>, loaded: Option<f64>, jitter: Option<f64>) -> Line {
    let Some(up) = up else {
        return line("Calls", Health::Fair, "Upload not measured");
    };
    if up < 1.5 {
        return line("Calls", Health::Poor, "Upload too slow for video calls");
    }
    match loaded {
        Some(ms) if ms >= 200.0 => {
            return line(
                "Calls",
                Health::Poor,
                "Calls will lag when the line is busy",
            )
        }
        Some(ms) if ms >= 100.0 => {
            return line(
                "Calls",
                Health::Fair,
                format!("Calls may stutter: latency jumps to {ms:.0} ms under load"),
            )
        }
        _ => {}
    }
    if let Some(ms) = jitter.filter(|ms| *ms >= 30.0) {
        return line(
            "Calls",
            Health::Fair,
            format!("Calls may sound choppy: jitter is {ms:.0} ms"),
        );
    }
    if up >= 4.0 && down >= 4.0 {
        line("Calls", Health::Good, "Good for video calls")
    } else {
        line(
            "Calls",
            Health::Fair,
            "Fine for calls at lower video quality",
        )
    }
}

fn games(idle: f64, increase: Option<f64>) -> Line {
    let increase = increase.unwrap_or(0.0);
    if idle < 20.0 && increase < 20.0 {
        line("Games", Health::Good, "Good for competitive games")
    } else if idle < 50.0 && increase < 50.0 {
        line("Games", Health::Good, "Fine for online games")
    } else {
        line("Games", Health::Fair, "Online games may lag")
    }
}

fn load(increase: f64) -> Line {
    match increase {
        ms if ms < 30.0 => line("Under load", Health::Good, "Steady under load"),
        ms if ms < 100.0 => line(
            "Under load",
            Health::Fair,
            format!("Slows under load: +{ms:.0} ms"),
        ),
        ms => line(
            "Under load",
            Health::Poor,
            format!("Lags under load: +{ms:.0} ms"),
        ),
    }
}

/// Plain sentences for a result, most limiting first in the headline.
pub fn meaning(report: &Report) -> Meaning {
    let down = report.download.mbps;
    let up = report.upload.as_ref().map(|speed| speed.mbps);
    let loaded = report.loaded_ms();
    let idle = report.latency.as_ref().map(|latency| latency.median_ms);
    let jitter = report
        .latency
        .as_ref()
        .and_then(|latency| latency.jitter_ms);
    let increase = idle
        .zip(loaded)
        .map(|(idle, loaded)| (loaded - idle).max(0.0));
    let mut lines = vec![video(down), calls(down, up, loaded, jitter)];
    if let Some(idle) = idle {
        lines.push(games(idle, increase));
    }
    if let Some(increase) = increase {
        lines.push(load(increase));
    }
    let health = health(report);
    let headline = match health {
        Health::Good => {
            let video = if down >= 25.0 { "4K video" } else { "HD video" };
            format!("Good for {video} and calls")
        }
        _ => {
            let problem = |line: &&Line| line.health != Health::Good;
            let activity = lines
                .iter()
                .filter(|line| line.topic != "Under load")
                .filter(problem)
                .min_by_key(|line| line.health != Health::Poor);
            activity
                .or_else(|| lines.iter().find(problem))
                .map_or_else(|| "Fine for everyday use".into(), |line| line.text.clone())
        }
    };
    Meaning {
        health,
        headline,
        lines,
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
            provider: "Cloudflare".into(),
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
            scheduled: false,
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

    fn texts(report: &Report) -> (String, Vec<String>) {
        let meaning = meaning(report);
        (
            meaning.headline,
            meaning.lines.into_iter().map(|line| line.text).collect(),
        )
    }

    #[test]
    fn good_line_reads_as_one_sentence() {
        let (headline, lines) = texts(&report(312.0, Some(41.0), Some(32.0), 2.0));
        assert_eq!(headline, "Good for 4K video and calls");
        assert_eq!(
            lines,
            [
                "Good for 4K video",
                "Good for video calls",
                "Good for competitive games",
                "Steady under load"
            ]
        );
    }

    #[test]
    fn headline_names_the_problem() {
        let (headline, _) = texts(&report(300.0, Some(40.0), Some(180.0), 2.0));
        assert_eq!(
            headline,
            "Calls may stutter: latency jumps to 180 ms under load"
        );
        let (headline, _) = texts(&report(300.0, Some(40.0), Some(240.0), 2.0));
        assert_eq!(headline, "Calls will lag when the line is busy");
        let (headline, _) = texts(&report(2.0, Some(1.0), Some(40.0), 2.0));
        assert_eq!(headline, "Video may buffer");
        let (headline, _) = texts(&report(300.0, Some(40.0), Some(40.0), 45.0));
        assert_eq!(headline, "Calls may sound choppy: jitter is 45 ms");
    }

    #[test]
    fn video_bands_follow_netflix_guidance() {
        for (down, text) in [
            (25.0, "Good for 4K video"),
            (15.0, "4K on one screen"),
            (5.0, "Good for HD video"),
            (3.0, "720p video"),
            (2.9, "Video may buffer"),
        ] {
            assert_eq!(video(down).text, text);
        }
    }

    #[test]
    fn games_and_load_use_the_increase_under_load() {
        assert_eq!(games(15.0, Some(10.0)).text, "Good for competitive games");
        assert_eq!(games(15.0, Some(30.0)).text, "Fine for online games");
        assert_eq!(games(60.0, Some(10.0)).text, "Online games may lag");
        assert_eq!(load(29.0).text, "Steady under load");
        assert_eq!(load(48.0).text, "Slows under load: +48 ms");
        assert_eq!(load(120.0).text, "Lags under load: +120 ms");
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
