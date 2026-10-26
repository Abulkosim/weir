//! A PDF for an internet provider: the last 30 days of tests against the plan,
//! slow hours, outages and every test. Contents: docs/method.md, "Provider report".

mod canvas;
#[cfg(test)]
mod tests;

use super::{
    export::error_text,
    history::Entry,
    meaning::Health,
    plan::{self, Plan},
    speedtest::{Report, TestError},
    stats::median,
};
use canvas::{
    Font, Page, Rgb, ACCENT, DANGER, HEIGHT, INK, LINE, MARGIN, SECONDARY, SOFT, TERTIARY, WARNING,
    WIDTH,
};
use chrono::{DateTime, FixedOffset, Timelike};

pub const DAYS: u64 = 30;
const DAY_MS: u64 = 86_400_000;
/// Results needed, over at least `MIN_HOURS` hours of the day, before naming slow hours.
const MIN_TESTS: usize = 10;
const MIN_HOURS: usize = 3;
const SLOW_SHARE: f64 = 0.8;

pub struct Options<'a> {
    pub now_ms: u64,
    pub offset: FixedOffset,
    pub plan: Plan,
    /// The outage watch is on now, so a period without outages means none were seen.
    pub watching: bool,
    pub version: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spread {
    pub median: f64,
    pub low: f64,
    pub high: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlowHours {
    pub from: u32,
    pub to: u32,
    pub median: f64,
    pub usual: f64,
}

#[derive(Debug, PartialEq)]
pub struct Summary<'a> {
    pub from_ms: u64,
    pub to_ms: u64,
    /// Newest first, as in History.
    pub entries: Vec<&'a Entry>,
    pub tests: usize,
    pub failed: Vec<(u64, TestError)>,
    pub download: Option<Spread>,
    pub upload: Option<Spread>,
    pub latency_ms: Option<f64>,
    pub loaded_ms: Option<f64>,
    /// Tests whose download was under half the plan.
    pub under_half: Option<usize>,
    pub slow: Option<SlowHours>,
    /// Hours of the day with results, so "no hour stands out" is only said with enough data.
    pub enough_for_hours: bool,
    pub outages: Vec<(u64, u64)>,
}

fn spread(values: &[f64]) -> Option<Spread> {
    Some(Spread {
        median: median(values)?,
        low: values.iter().copied().fold(f64::INFINITY, f64::min),
        high: values.iter().copied().fold(0.0, f64::max),
    })
}

/// The slowest run of hours by median download, as in History. `None` when no hour stands out.
pub fn slow_hours(reports: &[&Report], hour_of: impl Fn(u64) -> u32) -> (bool, Option<SlowHours>) {
    let mut by_hour: Vec<Vec<f64>> = vec![Vec::new(); 24];
    for report in reports {
        by_hour[hour_of(report.started_at_ms) as usize % 24].push(report.download.mbps);
    }
    let covered = by_hour.iter().filter(|values| !values.is_empty()).count();
    let all: Vec<f64> = reports.iter().map(|report| report.download.mbps).collect();
    let Some(usual) = median(&all) else {
        return (false, None);
    };
    if reports.len() < MIN_TESTS || covered < MIN_HOURS {
        return (false, None);
    }
    let mut slow: Option<SlowHours> = None;
    let mut run: Vec<u32> = Vec::new();
    let mut finish = |run: &mut Vec<u32>| {
        let (Some(&first), Some(&last)) = (run.first(), run.last()) else {
            return;
        };
        let values: Vec<f64> = run
            .iter()
            .flat_map(|hour| by_hour[*hour as usize].iter().copied())
            .collect();
        if let Some(value) = median(&values) {
            if slow.is_none_or(|slow| value < slow.median) {
                slow = Some(SlowHours {
                    from: first,
                    to: (last + 1) % 24,
                    median: value,
                    usual,
                });
            }
        }
        run.clear();
    };
    for hour in 0..24_u32 {
        match median(&by_hour[hour as usize]) {
            Some(value) if value < usual * SLOW_SHARE => run.push(hour),
            Some(_) => finish(&mut run),
            None => {}
        }
    }
    finish(&mut run);
    (true, slow)
}

fn local(ms: u64, offset: &FixedOffset) -> DateTime<FixedOffset> {
    DateTime::from_timestamp_millis(ms as i64)
        .unwrap_or_default()
        .with_timezone(offset)
}

pub fn summarize<'a>(entries: &'a [Entry], options: &Options) -> Option<Summary<'a>> {
    let from_ms = options.now_ms.saturating_sub(DAYS * DAY_MS);
    let mut period: Vec<&Entry> = entries
        .iter()
        .filter(|entry| match entry {
            Entry::Outage { end_ms, .. } => *end_ms >= from_ms,
            _ => entry.at_ms() >= from_ms,
        })
        .collect();
    if period.is_empty() {
        return None;
    }
    period.sort_by_key(|entry| std::cmp::Reverse(entry.at_ms()));
    let reports: Vec<&Report> = period.iter().filter_map(|entry| entry.report()).collect();
    let downloads: Vec<f64> = reports.iter().map(|report| report.download.mbps).collect();
    let uploads: Vec<f64> = reports
        .iter()
        .filter_map(|report| report.upload.as_ref().map(|speed| speed.mbps))
        .collect();
    let idle: Vec<f64> = reports
        .iter()
        .filter_map(|report| report.latency.as_ref().map(|latency| latency.median_ms))
        .collect();
    let loaded: Vec<f64> = reports
        .iter()
        .filter_map(|report| report.loaded_ms())
        .collect();
    let under_half = plan::valid(options.plan.download_mbps).map(|paid| {
        reports
            .iter()
            .filter(|report| {
                plan::share(report.download.mbps, paid, report.download.capped).health
                    == Health::Poor
            })
            .count()
    });
    let (enough_for_hours, slow) = slow_hours(&reports, |ms| local(ms, &options.offset).hour());
    let mut failed = Vec::new();
    let mut outages = Vec::new();
    for entry in &period {
        match entry {
            Entry::Failed { at_ms, error } => failed.push((*at_ms, *error)),
            Entry::Outage { start_ms, end_ms } => outages.push((*start_ms, *end_ms)),
            Entry::Result { .. } => {}
        }
    }
    Some(Summary {
        from_ms,
        to_ms: options.now_ms,
        tests: reports.len(),
        failed,
        download: spread(&downloads),
        upload: spread(&uploads),
        latency_ms: median(&idle),
        loaded_ms: median(&loaded),
        under_half,
        slow,
        enough_for_hours,
        outages,
        entries: period,
    })
}

pub fn mbps(value: f64) -> String {
    if value >= 10.0 {
        format!("{value:.0}")
    } else if value >= 1.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.2}")
    }
}

fn ms(value: f64) -> String {
    if value < 1.0 {
        "<1".into()
    } else {
        format!("{value:.0}")
    }
}

fn minutes(start_ms: u64, end_ms: u64) -> u64 {
    end_ms.saturating_sub(start_ms).div_ceil(60_000).max(1)
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// Plain sentences for the "What this shows" section.
pub fn findings(summary: &Summary, options: &Options) -> Vec<String> {
    let offset = &options.offset;
    let day = |ms: u64| local(ms, offset).format("%-d %b").to_string();
    let clock = |ms: u64| local(ms, offset).format("%H:%M").to_string();
    let mut lines = Vec::new();
    if let (Some(paid), Some(under)) = (plan::valid(options.plan.download_mbps), summary.under_half)
    {
        if summary.tests > 0 {
            lines.push(if under == 0 {
                format!("Download reached at least half of the {paid} Mbps plan in every test.")
            } else {
                format!(
                    "Download was under half of the {paid} Mbps plan in {under} of {}.",
                    plural(summary.tests, "test", "tests")
                )
            });
        }
    }
    match summary.slow {
        Some(slow) => lines.push(format!(
            "Slowest around {:02}:00–{:02}:00: {} Mbps median, against {} Mbps over the whole period.",
            slow.from,
            slow.to,
            mbps(slow.median),
            mbps(slow.usual)
        )),
        None if summary.enough_for_hours => {
            lines.push("No hour of the day stands out as slower.".into())
        }
        None => {}
    }
    if summary.outages.is_empty() {
        lines.push(if options.watching {
            "No outages recorded. Weir checked the connection about once a minute.".into()
        } else {
            "Outages were not monitored, so drops between tests are not included.".into()
        });
    } else {
        let total: u64 = summary
            .outages
            .iter()
            .map(|(start, end)| minutes(*start, *end))
            .sum();
        let (start, end) = summary
            .outages
            .iter()
            .copied()
            .max_by_key(|(start, end)| end.saturating_sub(*start))
            .unwrap_or_default();
        let count = match summary.outages.len() {
            1 => "once".to_owned(),
            count => format!("{count} times"),
        };
        lines.push(format!(
            "No internet {count}, {total} min in all. Longest: {}, {}–{} ({} min).",
            day(start),
            clock(start),
            clock(end),
            minutes(start, end)
        ));
    }
    match summary.failed.as_slice() {
        [] => {}
        [(at, error)] => lines.push(format!(
            "1 test failed: {} ({}, {}).",
            error_text(*error),
            day(*at),
            clock(*at)
        )),
        failed => lines.push(format!(
            "{} tests failed. Each is listed below with its reason.",
            failed.len()
        )),
    }
    lines
}

fn health_color(health: Health) -> Rgb {
    match health {
        Health::Good => ACCENT,
        Health::Fair => WARNING,
        Health::Poor => DANGER,
    }
}

const FOOTER_Y: f32 = HEIGHT - 30.0;
const BOTTOM: f32 = HEIGHT - 56.0;
const RIGHT: f32 = WIDTH - MARGIN;

struct Writer {
    pages: Vec<Page>,
    y: f32,
}

impl Writer {
    fn page(&mut self) -> &mut Page {
        self.pages.last_mut().expect("a page")
    }

    fn new_page(&mut self) {
        self.pages.push(Page::new());
        self.y = MARGIN + 8.0;
    }

    /// Starts a new page unless `height` more points fit on this one.
    fn room(&mut self, height: f32) -> bool {
        if self.y + height > BOTTOM {
            self.new_page();
            return false;
        }
        true
    }

    fn heading(&mut self, text: &str) {
        self.room(40.0);
        self.y += 6.0;
        let y = self.y;
        self.page().text(MARGIN, y, text, Font::Bold, 12.0, INK);
        self.y += 14.0;
    }
}

fn tile(
    page: &mut Page,
    x: f32,
    y: f32,
    label: &str,
    value: &str,
    unit: &str,
    notes: &[(String, Rgb)],
) {
    page.text(x, y, label, Font::Regular, 9.0, SECONDARY);
    page.text(x, y + 25.0, value, Font::Bold, 22.0, INK);
    if !unit.is_empty() {
        let after = x + canvas::width(value, Font::Bold, 22.0) + 3.0;
        page.text(after, y + 25.0, unit, Font::Regular, 10.0, SECONDARY);
    }
    for (index, (note, color)) in notes.iter().enumerate() {
        let note = canvas::fit(note, Font::Regular, 8.5, 116.0);
        page.text(
            x,
            y + 40.0 + index as f32 * 11.5,
            &note,
            Font::Regular,
            8.5,
            *color,
        );
    }
}

fn tiles(writer: &mut Writer, summary: &Summary, options: &Options) {
    let column = (RIGHT - MARGIN) / 4.0;
    let y = writer.y + 10.0;
    let share_note = |spread: Option<Spread>, paid: Option<u32>| {
        spread.zip(plan::valid(paid)).map(|(spread, paid)| {
            let share = plan::share(spread.median, paid, false);
            (
                format!("{}% of the {paid} Mbps plan", share.percent),
                health_color(share.health),
            )
        })
    };
    let range = |spread: Spread| {
        (
            format!("Range {}–{} Mbps", mbps(spread.low), mbps(spread.high)),
            TERTIARY,
        )
    };
    let page = writer.page();
    let speed = |spread: Option<Spread>, paid: Option<u32>| -> (String, Vec<(String, Rgb)>) {
        match spread {
            Some(value) => (
                mbps(value.median),
                std::iter::once(range(value))
                    .chain(share_note(spread, paid))
                    .collect(),
            ),
            None => ("–".into(), vec![("Not measured".into(), TERTIARY)]),
        }
    };
    let (down, down_notes) = speed(summary.download, options.plan.download_mbps);
    tile(
        page,
        MARGIN,
        y,
        "Download, median",
        &down,
        if summary.download.is_some() {
            "Mbps"
        } else {
            ""
        },
        &down_notes,
    );
    let (up, up_notes) = speed(summary.upload, options.plan.upload_mbps);
    tile(
        page,
        MARGIN + column,
        y,
        "Upload, median",
        &up,
        if summary.upload.is_some() { "Mbps" } else { "" },
        &up_notes,
    );
    let latency_notes = vec![(
        summary.loaded_ms.map_or_else(
            || "Not measured under load".into(),
            |value| format!("{} ms under load", ms(value)),
        ),
        TERTIARY,
    )];
    match summary.latency_ms {
        Some(value) => tile(
            page,
            MARGIN + column * 2.0,
            y,
            "Latency, median",
            &ms(value),
            "ms",
            &latency_notes,
        ),
        None => tile(
            page,
            MARGIN + column * 2.0,
            y,
            "Latency, median",
            "–",
            "",
            &latency_notes,
        ),
    }
    let failed = match summary.failed.len() {
        0 => ("None failed".to_owned(), TERTIARY),
        count => (format!("{count} failed"), DANGER),
    };
    tile(
        page,
        MARGIN + column * 3.0,
        y,
        "Tests",
        &summary.tests.to_string(),
        "",
        &[failed],
    );
    writer.y = y + 72.0;
}

fn nice_top(max: f64) -> f64 {
    let target = (max * 1.1).max(1.0);
    let scale = 10_f64.powf(target.log10().floor());
    [1.0, 1.2, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0]
        .into_iter()
        .map(|step| step * scale)
        .find(|top| *top >= target)
        .unwrap_or(10.0 * scale)
}

fn chart(writer: &mut Writer, summary: &Summary, options: &Options) {
    let reports: Vec<&Report> = summary
        .entries
        .iter()
        .rev()
        .filter_map(|entry| entry.report())
        .collect();
    if reports.is_empty() && summary.outages.is_empty() {
        return;
    }
    writer.heading("Download by test, Mbps");
    let paid = plan::valid(options.plan.download_mbps);
    let max = reports
        .iter()
        .map(|report| report.download.mbps)
        .fold(f64::from(paid.unwrap_or(0)), f64::max);
    let top = nice_top(max);
    let (left, right) = (MARGIN + 30.0, RIGHT);
    let (chart_top, height) = (writer.y + 8.0, 130.0_f32);
    let bottom = chart_top + height;
    let span = summary.to_ms.saturating_sub(summary.from_ms).max(1) as f32;
    let x = |ms: u64| {
        left + (ms.saturating_sub(summary.from_ms) as f32 / span).min(1.0) * (right - left)
    };
    let y = |value: f64| bottom - (value / top) as f32 * height;
    let page = writer.page();
    for fraction in [0.0, 0.5, 1.0] {
        let value = top * fraction;
        page.line((left, y(value)), (right, y(value)), 0.6, LINE, false);
        let label = if value == 0.0 {
            "0".into()
        } else {
            mbps(value)
        };
        page.text_right(
            left - 6.0,
            y(value) + 3.0,
            &label,
            Font::Regular,
            8.0,
            TERTIARY,
        );
    }
    if let Some(paid) = paid {
        let level = y(f64::from(paid));
        page.line((left, level), (right, level), 0.8, TERTIARY, true);
        page.text_right(
            right,
            level - 4.0,
            &format!("Plan {paid} Mbps"),
            Font::Regular,
            8.0,
            SECONDARY,
        );
    }
    for (start, end) in &summary.outages {
        let (from, to) = (x(*start), x(*end));
        page.rect(from, bottom - 5.0, (to - from).max(1.5), 5.0, DANGER);
    }
    let points: Vec<(f32, f32)> = reports
        .iter()
        .map(|report| (x(report.started_at_ms), y(report.download.mbps)))
        .collect();
    page.polyline(&points, 1.1, ACCENT);
    for (report, point) in reports.iter().zip(&points) {
        let under_half = paid.is_some_and(|paid| {
            plan::share(report.download.mbps, paid, report.download.capped).health == Health::Poor
        });
        let color = if under_half { DANGER } else { ACCENT };
        page.dot(*point, 2.2, color);
    }
    let offset = &options.offset;
    let date = |ms: u64| local(ms, offset).format("%-d %b").to_string();
    let label_y = bottom + 13.0;
    page.text(
        left,
        label_y,
        &date(summary.from_ms),
        Font::Regular,
        8.0,
        TERTIARY,
    );
    let middle = summary.from_ms + (summary.to_ms - summary.from_ms) / 2;
    let middle_label = date(middle);
    let middle_x = x(middle) - canvas::width(&middle_label, Font::Regular, 8.0) / 2.0;
    page.text(
        middle_x,
        label_y,
        &middle_label,
        Font::Regular,
        8.0,
        TERTIARY,
    );
    page.text_right(
        right,
        label_y,
        &date(summary.to_ms),
        Font::Regular,
        8.0,
        TERTIARY,
    );
    let mut legend_x = left;
    let legend_y = label_y + 16.0;
    page.dot((legend_x + 3.0, legend_y - 3.0), 2.2, ACCENT);
    page.text(
        legend_x + 9.0,
        legend_y,
        "A test",
        Font::Regular,
        8.0,
        SECONDARY,
    );
    legend_x += 52.0;
    if paid.is_some() {
        page.dot((legend_x + 3.0, legend_y - 3.0), 2.2, DANGER);
        page.text(
            legend_x + 9.0,
            legend_y,
            "Under half the plan",
            Font::Regular,
            8.0,
            SECONDARY,
        );
        legend_x += 100.0;
    }
    if !summary.outages.is_empty() {
        page.rect(legend_x, legend_y - 6.0, 7.0, 5.0, DANGER);
        page.text(
            legend_x + 11.0,
            legend_y,
            "No internet",
            Font::Regular,
            8.0,
            SECONDARY,
        );
    }
    writer.y = legend_y + 18.0;
}

/// Right edges of the numeric columns, and where Server starts.
const COLUMNS: [f32; 4] = [238.0, 306.0, 374.0, 448.0];
const SERVER_X: f32 = 466.0;
const ROW: f32 = 17.0;

fn table_header(writer: &mut Writer) {
    let y = writer.y + 4.0;
    let page = writer.page();
    page.text(MARGIN, y, "Date", Font::Bold, 8.5, SECONDARY);
    page.text(126.0, y, "Time", Font::Bold, 8.5, SECONDARY);
    for (right, (label, unit)) in COLUMNS.iter().zip([
        ("Download", "Mbps"),
        ("Upload", "Mbps"),
        ("Latency", "ms"),
        ("Under load", "ms"),
    ]) {
        page.text_right(*right, y, label, Font::Bold, 8.5, SECONDARY);
        page.text_right(*right, y + 10.0, unit, Font::Regular, 8.0, TERTIARY);
    }
    page.text(SERVER_X, y, "Server", Font::Bold, 8.5, SECONDARY);
    page.line((MARGIN, y + 16.0), (RIGHT, y + 16.0), 0.6, LINE, false);
    writer.y = y + 16.0;
}

fn table(writer: &mut Writer, summary: &Summary, options: &Options) {
    writer.heading("Every test");
    table_header(writer);
    let offset = &options.offset;
    for entry in &summary.entries {
        if !writer.room(ROW) {
            table_header(writer);
        }
        let y = writer.y + 12.0;
        let at = local(entry.at_ms(), offset);
        let page = writer.page();
        page.text(
            MARGIN,
            y,
            &at.format("%-d %b %Y").to_string(),
            Font::Regular,
            9.0,
            INK,
        );
        page.text(
            126.0,
            y,
            &at.format("%H:%M").to_string(),
            Font::Regular,
            9.0,
            INK,
        );
        match entry {
            Entry::Result { report } => {
                let cells = [
                    Some(mbps(report.download.mbps)),
                    report.upload.as_ref().map(|speed| mbps(speed.mbps)),
                    report.latency.as_ref().map(|latency| ms(latency.median_ms)),
                    report.loaded_ms().map(ms),
                ];
                for (right, cell) in COLUMNS.iter().zip(cells) {
                    let (text, color) =
                        cell.map_or(("–".to_owned(), TERTIARY), |text| (text, INK));
                    page.text_right(*right, y, &text, Font::Regular, 9.0, color);
                }
                let server = format!(
                    "{} {}",
                    report.provider,
                    report.colo.as_deref().unwrap_or("")
                );
                let server = canvas::fit(server.trim_end(), Font::Regular, 9.0, RIGHT - SERVER_X);
                page.text(SERVER_X, y, &server, Font::Regular, 9.0, SECONDARY);
            }
            Entry::Failed { error, .. } => {
                page.text(
                    170.0,
                    y,
                    &format!("Test failed: {}", error_text(*error)),
                    Font::Regular,
                    9.0,
                    DANGER,
                );
            }
            Entry::Outage { start_ms, end_ms } => {
                let end = local(*end_ms, offset);
                let until = if end.date_naive() == at.date_naive() {
                    end.format("%H:%M").to_string()
                } else {
                    end.format("%-d %b %H:%M").to_string()
                };
                page.text(
                    170.0,
                    y,
                    &format!(
                        "No internet until {until} · {} min",
                        minutes(*start_ms, *end_ms)
                    ),
                    Font::Regular,
                    9.0,
                    DANGER,
                );
            }
        }
        page.line((MARGIN, y + 5.0), (RIGHT, y + 5.0), 0.4, LINE, false);
        writer.y += ROW;
    }
}

const METHOD: &str = "Each test downloads and uploads over 6 parallel connections to the nearest Cloudflare server (speed.cloudflare.com), discards the warm-up and reports the steady average. Latency is the median round trip while the line is idle; under load is measured during the download and upload. Wi-Fi, other devices on the network and the server all affect a single test; tests on a cable are the strongest evidence.";

/// The report as PDF bytes, or `None` when the last 30 days have no tests or outages.
pub fn pdf(entries: &[Entry], options: &Options) -> Option<Vec<u8>> {
    let summary = summarize(entries, options)?;
    let mut writer = Writer {
        pages: Vec::new(),
        y: 0.0,
    };
    writer.new_page();
    let offset = &options.offset;
    let date = |ms: u64| local(ms, offset).format("%-d %b %Y").to_string();
    let page = writer.page();
    page.text(
        MARGIN,
        66.0,
        "Internet connection report",
        Font::Bold,
        22.0,
        INK,
    );
    page.text(
        MARGIN,
        86.0,
        &format!(
            "{} – {} · Last {DAYS} days · Times in UTC{}",
            date(summary.from_ms),
            date(summary.to_ms),
            offset
        ),
        Font::Regular,
        10.0,
        SECONDARY,
    );
    if let Some(paid) = plan::valid(options.plan.download_mbps) {
        let upload = plan::valid(options.plan.upload_mbps)
            .map_or_else(String::new, |up| format!(" down, {up} Mbps up"));
        page.text(
            MARGIN,
            100.0,
            &format!("Plan: {paid} Mbps{upload}, as entered in Weir"),
            Font::Regular,
            10.0,
            SECONDARY,
        );
    }
    writer.y = 112.0;
    tiles(&mut writer, &summary, options);
    let findings = findings(&summary, options);
    if !findings.is_empty() {
        writer.heading("What this shows");
        for finding in findings {
            let lines = canvas::wrap(&finding, Font::Regular, 10.0, RIGHT - MARGIN - 14.0);
            writer.room(lines.len() as f32 * 14.0 + 4.0);
            let start = writer.y + 11.0;
            let page = writer.page();
            page.dot((MARGIN + 3.0, start - 3.5), 1.6, SECONDARY);
            for (index, line) in lines.iter().enumerate() {
                page.text(
                    MARGIN + 14.0,
                    start + index as f32 * 14.0,
                    line,
                    Font::Regular,
                    10.0,
                    INK,
                );
            }
            writer.y += lines.len() as f32 * 14.0 + 4.0;
        }
        writer.y += 6.0;
    }
    chart(&mut writer, &summary, options);
    table(&mut writer, &summary, options);
    let method = canvas::wrap(METHOD, Font::Regular, 8.5, RIGHT - MARGIN - 24.0);
    let box_height = method.len() as f32 * 11.5 + 30.0;
    writer.room(box_height + 18.0);
    writer.y += 18.0;
    let top = writer.y;
    let page = writer.page();
    page.rect(MARGIN, top, RIGHT - MARGIN, box_height, SOFT);
    page.text(
        MARGIN + 12.0,
        top + 17.0,
        "How Weir measures",
        Font::Bold,
        9.0,
        INK,
    );
    for (index, line) in method.iter().enumerate() {
        page.text(
            MARGIN + 12.0,
            top + 31.0 + index as f32 * 11.5,
            line,
            Font::Regular,
            8.5,
            SECONDARY,
        );
    }
    let total = writer.pages.len();
    let footer = format!(
        "Made with Weir {} · github.com/Abulkosim/weir · No accounts, nothing uploaded",
        options.version
    );
    for (index, page) in writer.pages.iter_mut().enumerate() {
        page.text(MARGIN, FOOTER_Y, &footer, Font::Regular, 8.0, TERTIARY);
        page.text_right(
            RIGHT,
            FOOTER_Y,
            &format!("Page {} of {total}", index + 1),
            Font::Regular,
            8.0,
            TERTIARY,
        );
    }
    Some(canvas::document(
        writer.pages,
        "Internet connection report",
        &format!("Weir {}", options.version),
    ))
}
