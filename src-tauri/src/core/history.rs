//! Every test, kept locally for a year in one bounded JSON file.

use super::speedtest::{Report, TestError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub const VERSION: u32 = 1;
pub const RETENTION_MS: u64 = 366 * 24 * 60 * 60 * 1000;
pub const MAX_ENTRIES: usize = 5_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Entry {
    Result {
        report: Box<Report>,
    },
    #[serde(rename_all = "camelCase")]
    Failed {
        at_ms: u64,
        error: TestError,
    },
    #[serde(rename_all = "camelCase")]
    Outage {
        start_ms: u64,
        end_ms: u64,
    },
}

impl Entry {
    pub fn at_ms(&self) -> u64 {
        match self {
            Self::Result { report } => report.started_at_ms,
            Self::Failed { at_ms, .. } => *at_ms,
            Self::Outage { start_ms, .. } => *start_ms,
        }
    }

    pub fn report(&self) -> Option<&Report> {
        match self {
            Self::Result { report } => Some(report),
            Self::Failed { .. } | Self::Outage { .. } => None,
        }
    }
}

#[derive(Serialize)]
struct File<'a> {
    version: u32,
    entries: &'a [Entry],
}

#[derive(Debug)]
pub struct History {
    path: PathBuf,
    entries: Vec<Entry>,
    /// False when the file is from a newer Weir or couldn't be opened or moved
    /// aside: never overwrite it.
    writable: bool,
}

#[derive(Debug, PartialEq)]
pub enum Recovery {
    /// Some entries could not be read and were dropped.
    Skipped(usize),
    /// The file could not be read at all; it was moved aside.
    BackedUp(PathBuf),
    /// The file couldn't be opened, or couldn't be moved aside when unreadable.
    Unopened,
    /// Written by a newer Weir.
    Newer,
}

fn backup_path(path: &Path, now_ms: u64) -> PathBuf {
    path.with_file_name(format!("history.unreadable-{now_ms}.json"))
}

/// The entries that parse, oldest first.
fn readable(raw: Vec<Value>) -> Vec<Entry> {
    let mut entries: Vec<Entry> = raw
        .into_iter()
        .filter_map(|value| serde_json::from_value(value).ok())
        .collect();
    entries.sort_by_key(Entry::at_ms);
    entries
}

/// Reads entries without changing anything on disk, for the command line. An unreadable
/// file reads as empty; the app deals with it next time it starts.
pub fn read_only(path: &Path) -> Vec<Entry> {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| parse(&text))
        .map(|(entries, _)| entries)
        .unwrap_or_default()
}

/// The readable entries, oldest first, and how many were dropped. `None` when the
/// text is not a history file at all.
pub fn parse(text: &str) -> Option<(Vec<Entry>, usize)> {
    let raw = serde_json::from_str::<Value>(text)
        .ok()?
        .get("entries")?
        .as_array()?
        .clone();
    let total = raw.len();
    let entries = readable(raw);
    let skipped = total - entries.len();
    Some((entries, skipped))
}

/// Written by a newer Weir, which may store entries this one can't read.
fn is_newer(text: &str) -> bool {
    #[derive(Deserialize)]
    struct Header {
        version: Option<u64>,
    }
    serde_json::from_str::<Header>(text)
        .ok()
        .and_then(|header| header.version)
        .is_some_and(|version| version > u64::from(VERSION))
}

pub fn to_text(entries: &[Entry]) -> serde_json::Result<String> {
    serde_json::to_string(&File {
        version: VERSION,
        entries,
    })
}

impl History {
    pub fn load(path: PathBuf, now_ms: u64) -> (Self, Option<Recovery>) {
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return (Self::empty(path, true), None)
            }
            Err(_) => return (Self::empty(path, false), Some(Recovery::Unopened)),
        };
        let Some((entries, skipped)) = parse(&text) else {
            let backup = backup_path(&path, now_ms);
            return match fs::rename(&path, &backup) {
                Ok(()) => (Self::empty(path, true), Some(Recovery::BackedUp(backup))),
                Err(_) => (Self::empty(path, false), Some(Recovery::Unopened)),
            };
        };
        let newer = is_newer(&text);
        let mut history = Self {
            path,
            entries,
            writable: !newer,
        };
        history.prune(now_ms);
        let recovery = if newer {
            Some(Recovery::Newer)
        } else {
            (skipped > 0).then_some(Recovery::Skipped(skipped))
        };
        (history, recovery)
    }

    fn empty(path: PathBuf, writable: bool) -> Self {
        Self {
            path,
            entries: Vec::new(),
            writable,
        }
    }

    fn prune(&mut self, now_ms: u64) {
        let oldest = now_ms.saturating_sub(RETENTION_MS);
        self.entries.retain(|entry| entry.at_ms() >= oldest);
        let excess = self.entries.len().saturating_sub(MAX_ENTRIES);
        self.entries.drain(..excess);
    }

    pub fn push(&mut self, entry: Entry, now_ms: u64) -> io::Result<()> {
        let at = entry.at_ms();
        let index = self
            .entries
            .partition_point(|existing| existing.at_ms() <= at);
        self.entries.insert(index, entry);
        self.prune(now_ms);
        self.save()
    }

    pub fn clear(&mut self) -> io::Result<()> {
        self.entries.clear();
        self.save()
    }

    pub fn save(&self) -> io::Result<()> {
        if !self.writable {
            return Err(io::Error::other("history file is read-only"));
        }
        let text = to_text(&self.entries).map_err(io::Error::other)?;
        super::files::replace(&self.path, &text)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Oldest first.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Newest first, at most `limit`.
    pub fn recent(&self, limit: usize) -> Vec<Entry> {
        self.entries.iter().rev().take(limit).cloned().collect()
    }

    pub fn last_report(&self) -> Option<&Report> {
        self.entries.iter().rev().find_map(Entry::report)
    }

    pub fn last_failure(&self) -> Option<(u64, TestError)> {
        self.entries.iter().rev().find_map(|entry| match entry {
            Entry::Failed { at_ms, error } => Some((*at_ms, *error)),
            Entry::Result { .. } | Entry::Outage { .. } => None,
        })
    }

    /// `(down, up)` of recent results, oldest first, for the data estimate.
    pub fn speeds(&self, limit: usize) -> Vec<(f64, f64)> {
        let mut speeds: Vec<(f64, f64)> = self
            .entries
            .iter()
            .rev()
            .filter_map(Entry::report)
            .filter_map(|report| Some((report.download.mbps, report.upload.as_ref()?.mbps)))
            .take(limit)
            .collect();
        speeds.reverse();
        speeds
    }
}

#[cfg(test)]
mod tests;
