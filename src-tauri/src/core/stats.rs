//! Measurement math from docs/method.md. Pure functions only: no clocks, no I/O.

pub const BIN_MS: u64 = 250;
const WINDOW_BINS: usize = 4;
const MIN_STEADY_START_MS: u64 = 1_000;
const MAX_STEADY_START_MS: u64 = 4_000;
const RISE_TOLERANCE: f64 = 1.03;
const STALL_MS: u64 = 3_000;
const CONVERGE_SAMPLES: usize = 6;
const CONVERGE_SPREAD: f64 = 0.03;
const VARIED_SPREAD: f64 = 0.25;
pub const PROBE_TIMEOUT_MS: f64 = 2_000.0;

pub fn median(values: &[f64]) -> Option<f64> {
    percentile(values, 0.5)
}

/// Linear interpolation between closest ranks.
pub fn percentile(values: &[f64], p: f64) -> Option<f64> {
    let mut sorted: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(f64::total_cmp);
    let rank = p.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let low = rank.floor() as usize;
    let high = rank.ceil() as usize;
    Some(sorted[low] + (sorted[high] - sorted[low]) * (rank - low as f64))
}

/// Mean absolute difference of consecutive values (Cloudflare's jitter).
pub fn jitter(values: &[f64]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let total: f64 = values
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .sum();
    Some(total / (values.len() - 1) as f64)
}

/// Server processing time from a `Server-Timing` header: `cfReqDur` if present,
/// else the sum of every `cfSpeed*` duration.
pub fn server_time_ms(header: &str) -> Option<f64> {
    let mut request = None;
    let mut speed = 0.0;
    let mut found = false;
    for metric in header.split(',') {
        let mut parts = metric.split(';').map(str::trim);
        let name = parts.next().unwrap_or_default();
        let duration = parts
            .find_map(|part| part.strip_prefix("dur="))
            .and_then(|value| value.trim_matches('"').parse::<f64>().ok())
            .filter(|value| value.is_finite() && *value >= 0.0);
        let Some(duration) = duration else { continue };
        if name == "cfReqDur" {
            request = Some(duration);
        } else if name.starts_with("cfSpeed") {
            speed += duration;
            found = true;
        }
    }
    request.or(found.then_some(speed))
}

/// Round trip of a latency probe: time to first byte minus server time.
pub fn probe_rtt(ttfb_ms: f64, server_ms: Option<f64>) -> f64 {
    match server_ms {
        Some(server) if ttfb_ms - server > 1.0 => ttfb_ms - server,
        _ => ttfb_ms,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Latency {
    pub median_ms: f64,
    pub jitter_ms: Option<f64>,
    pub answered: usize,
    pub sent: usize,
}

/// Idle latency from probes in send order; `None` is a probe that did not answer.
/// The first probe warms the connection and is dropped.
pub fn idle_latency(probes: &[Option<f64>]) -> Option<Latency> {
    let counted = probes.get(1..)?;
    let answered: Vec<f64> = counted.iter().flatten().copied().collect();
    if answered.len() < 10 {
        return None;
    }
    Some(Latency {
        median_ms: median(&answered)?,
        jitter_ms: jitter(&answered),
        answered: answered.len(),
        sent: counted.len(),
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Probe {
    pub sent_ms: u64,
    pub rtt_ms: Option<f64>,
}

/// Median latency of probes sent inside the steady window. A timed-out probe
/// counts as the timeout so bufferbloat is not hidden.
pub fn loaded_latency(probes: &[Probe], start_ms: u64, end_ms: u64) -> Option<f64> {
    let counted: Vec<f64> = probes
        .iter()
        .filter(|probe| probe.sent_ms >= start_ms && probe.sent_ms < end_ms)
        .map(|probe| {
            probe
                .rtt_ms
                .unwrap_or(PROBE_TIMEOUT_MS)
                .min(PROBE_TIMEOUT_MS)
        })
        .collect();
    if counted.len() < 5 {
        return None;
    }
    median(&counted)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    pub at_ms: u64,
    pub bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    pub min_ms: u64,
    pub max_ms: u64,
    pub max_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    Converged,
    Time,
    Cap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    TooShort,
    Stalled,
    Dropped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Continue,
    Stop(Stop),
    Fail(Failure),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Throughput {
    pub mbps: f64,
    pub p10: f64,
    pub p50: f64,
    pub p90: f64,
    pub steady_start_ms: u64,
    pub steady_ms: u64,
    pub bytes: u64,
    pub ramped_slowly: bool,
    pub capped: bool,
}

impl Throughput {
    pub fn varied(&self) -> bool {
        self.mbps > 0.0 && (self.p90 - self.p10) / self.mbps > VARIED_SPREAD
    }
}

fn mbps(bytes: f64, ms: f64) -> f64 {
    if ms <= 0.0 {
        0.0
    } else {
        bytes * 8.0 / ms / 1_000.0
    }
}

/// Byte counter readings for one direction, fed as the test runs.
#[derive(Clone, Debug, Default)]
pub struct Tracker {
    readings: Vec<Reading>,
    last_growth_ms: u64,
}

impl Tracker {
    pub fn new() -> Self {
        Self {
            readings: vec![Reading { at_ms: 0, bytes: 0 }],
            last_growth_ms: 0,
        }
    }

    pub fn from_readings(readings: &[Reading]) -> Self {
        let mut tracker = Self::new();
        for reading in readings {
            tracker.push(*reading);
        }
        tracker
    }

    pub fn push(&mut self, reading: Reading) {
        let last = *self.readings.last().expect("tracker starts with a reading");
        if reading.at_ms <= last.at_ms {
            return;
        }
        let bytes = reading.bytes.max(last.bytes);
        if bytes > last.bytes {
            self.last_growth_ms = reading.at_ms;
        }
        self.readings.push(Reading {
            at_ms: reading.at_ms,
            bytes,
        });
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.readings.last().map_or(0, |reading| reading.at_ms)
    }

    pub fn bytes(&self) -> u64 {
        self.readings.last().map_or(0, |reading| reading.bytes)
    }

    fn bytes_at(&self, at_ms: u64) -> f64 {
        let index = self
            .readings
            .partition_point(|reading| reading.at_ms <= at_ms);
        let Some(before) = index.checked_sub(1).map(|i| self.readings[i]) else {
            return 0.0;
        };
        let Some(after) = self.readings.get(index) else {
            return before.bytes as f64;
        };
        let span = (after.at_ms - before.at_ms) as f64;
        let part = (at_ms - before.at_ms) as f64 / span;
        before.bytes as f64 + (after.bytes - before.bytes) as f64 * part
    }

    /// Rate of each complete 250 ms bin, in Mbps.
    pub fn bins(&self) -> Vec<f64> {
        let complete = self.elapsed_ms() / BIN_MS;
        (0..complete)
            .map(|bin| {
                let start = bin * BIN_MS;
                let bytes = self.bytes_at(start + BIN_MS) - self.bytes_at(start);
                mbps(bytes, BIN_MS as f64)
            })
            .collect()
    }

    /// One-second moving average for the live gauge.
    pub fn live_mbps(&self) -> f64 {
        let end = self.elapsed_ms();
        let start = end.saturating_sub(WINDOW_BINS as u64 * BIN_MS);
        mbps(
            self.bytes_at(end) - self.bytes_at(start),
            (end - start) as f64,
        )
    }

    /// Start of the steady window: the 1 s moving average stops rising for two
    /// bins in a row, between 1 s and 4 s. `(start, ramped_slowly)`.
    pub fn steady_start(&self) -> Option<(u64, bool)> {
        let bins = self.bins();
        let averages: Vec<f64> = (WINDOW_BINS - 1..bins.len())
            .map(|end| bins[end + 1 - WINDOW_BINS..=end].iter().sum::<f64>() / WINDOW_BINS as f64)
            .collect();
        let mut best = averages.first().copied()?;
        let mut flat_since: Option<usize> = None;
        for (offset, average) in averages.iter().enumerate().skip(1) {
            let bin = offset + WINDOW_BINS - 1;
            let start_ms = bin as u64 * BIN_MS;
            if start_ms > MAX_STEADY_START_MS {
                break;
            }
            if *average >= best * RISE_TOLERANCE {
                flat_since = None;
            } else if let Some(first) = flat_since {
                let start = (first as u64 * BIN_MS).max(MIN_STEADY_START_MS);
                return Some((start, false));
            } else {
                flat_since = Some(bin);
            }
            best = best.max(*average);
        }
        (self.elapsed_ms() >= MAX_STEADY_START_MS + BIN_MS).then_some((MAX_STEADY_START_MS, true))
    }

    pub fn decide(&self, limits: Limits, streams_alive: usize, streams: usize) -> Decision {
        let elapsed = self.elapsed_ms();
        if streams_alive * 2 < streams {
            return Decision::Fail(Failure::Dropped);
        }
        if elapsed.saturating_sub(self.last_growth_ms) >= STALL_MS {
            return Decision::Fail(Failure::Stalled);
        }
        if self.bytes() >= limits.max_bytes {
            return Decision::Stop(Stop::Cap);
        }
        if elapsed >= limits.max_ms {
            return Decision::Stop(Stop::Time);
        }
        if elapsed < limits.min_ms {
            return Decision::Continue;
        }
        let Some((start, _)) = self.steady_start() else {
            return Decision::Continue;
        };
        let bins = self.bins();
        let first = (start / BIN_MS) as usize;
        if bins.len() < first + 12 {
            return Decision::Continue;
        }
        let steady = &bins[first..];
        let mut sum = 0.0;
        let running: Vec<f64> = steady
            .iter()
            .enumerate()
            .map(|(index, value)| {
                sum += value;
                sum / (index + 1) as f64
            })
            .collect();
        let recent = &running[running.len() - CONVERGE_SAMPLES..];
        let high = recent.iter().copied().fold(f64::MIN, f64::max);
        let low = recent.iter().copied().fold(f64::MAX, f64::min);
        let current = *recent.last().expect("six running means");
        if current > 0.0 && (high - low) / current < CONVERGE_SPREAD {
            Decision::Stop(Stop::Converged)
        } else {
            Decision::Continue
        }
    }

    /// Final number for a stopped direction, or why there is none.
    pub fn result(&self, stop: Stop) -> Result<Throughput, Failure> {
        let end = self.elapsed_ms();
        let capped = stop == Stop::Cap;
        let (start, ramped_slowly) = match self.steady_start() {
            Some(found) => found,
            None if capped && end >= MIN_STEADY_START_MS * 2 => (MIN_STEADY_START_MS, true),
            None => return Err(Failure::TooShort),
        };
        let steady_ms = end.saturating_sub(start);
        let needed = if capped { 1_000 } else { 2_000 };
        if steady_ms < needed {
            return Err(Failure::TooShort);
        }
        let rate = mbps(self.bytes_at(end) - self.bytes_at(start), steady_ms as f64);
        if rate <= 0.0 {
            return Err(Failure::Stalled);
        }
        let bins = self.bins();
        let steady = &bins[((start / BIN_MS) as usize).min(bins.len())..];
        let spread = |p| percentile(steady, p).unwrap_or(rate);
        Ok(Throughput {
            mbps: rate,
            p10: spread(0.1),
            p50: spread(0.5),
            p90: spread(0.9),
            steady_start_ms: start,
            steady_ms,
            bytes: self.bytes(),
            ramped_slowly,
            capped,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Completed {
    pub stream: usize,
    pub started_ms: u64,
    pub bytes: u64,
    pub duration_ms: f64,
}

/// Upload rate from completed requests that started in the steady window.
/// `None` when any stream finished fewer than three such requests.
pub fn upload_mbps(requests: &[Completed], streams: usize, steady_start_ms: u64) -> Option<f64> {
    let mut total = 0.0;
    for stream in 0..streams {
        let mine: Vec<&Completed> = requests
            .iter()
            .filter(|request| {
                request.stream == stream
                    && request.started_ms >= steady_start_ms
                    && request.duration_ms > 0.0
            })
            .collect();
        if mine.len() < 3 {
            return None;
        }
        let bytes: u64 = mine.iter().map(|request| request.bytes).sum();
        let ms: f64 = mine.iter().map(|request| request.duration_ms).sum();
        total += mbps(bytes as f64, ms);
    }
    (total > 0.0).then_some(total)
}

pub const DOWNLOAD_CAP_MB: f64 = 380.0;
pub const UPLOAD_CAP_MB: f64 = 190.0;
const PROBES_MB: f64 = 1.0;
const WIRE_OVERHEAD: f64 = 1.05;
const DOWNLOAD_SECONDS: f64 = 7.0;
const UPLOAD_SECONDS: f64 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataEstimate {
    UpTo(u32),
    About(u32),
}

fn round_up(mb: f64) -> u32 {
    let step = if mb < 50.0 { 5.0 } else { 10.0 };
    ((mb / step).ceil() * step).max(step) as u32
}

/// Data a test will use, from recent results on this network `(down, up)` in Mbps.
pub fn data_estimate(recent: &[(f64, f64)]) -> DataEstimate {
    let last: Vec<&(f64, f64)> = recent.iter().rev().take(3).collect();
    let down = median(&last.iter().map(|pair| pair.0).collect::<Vec<_>>());
    let up = median(&last.iter().map(|pair| pair.1).collect::<Vec<_>>());
    match (down, up) {
        (Some(down), Some(up)) if down > 0.0 && up > 0.0 => {
            let mb = ((down / 8.0 * DOWNLOAD_SECONDS).min(DOWNLOAD_CAP_MB)
                + (up / 8.0 * UPLOAD_SECONDS).min(UPLOAD_CAP_MB)
                + PROBES_MB)
                * WIRE_OVERHEAD;
            DataEstimate::About(round_up(mb))
        }
        _ => DataEstimate::UpTo(
            ((DOWNLOAD_CAP_MB + UPLOAD_CAP_MB + PROBES_MB) * WIRE_OVERHEAD / 50.0).round() as u32
                * 50,
        ),
    }
}

#[cfg(test)]
mod tests;
