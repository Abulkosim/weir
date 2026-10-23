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
}

impl Entry {
    pub fn at_ms(&self) -> u64 {
        match self {
            Self::Result { report } => report.started_at_ms,
            Self::Failed { at_ms, .. } => *at_ms,
        }
    }

    pub fn report(&self) -> Option<&Report> {
        match self {
            Self::Result { report } => Some(report),
            Self::Failed { .. } => None,
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
}

#[derive(Debug, PartialEq)]
pub enum Recovery {
    /// Some entries could not be read and were dropped.
    Skipped(usize),
    /// The file could not be read at all; it was moved aside.
    BackedUp(PathBuf),
}

fn backup_path(path: &Path, now_ms: u64) -> PathBuf {
    path.with_file_name(format!("history.unreadable-{now_ms}.json"))
}

impl History {
    pub fn load(path: PathBuf, now_ms: u64) -> (Self, Option<Recovery>) {
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(_) => return (Self::empty(path), None),
        };
        let raw = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|value| value.get("entries").and_then(Value::as_array).cloned());
        let Some(raw) = raw else {
            let backup = backup_path(&path, now_ms);
            let recovery = fs::rename(&path, &backup)
                .ok()
                .map(|_| Recovery::BackedUp(backup));
            return (Self::empty(path), recovery);
        };
        let total = raw.len();
        let mut entries: Vec<Entry> = raw
            .into_iter()
            .filter_map(|value| serde_json::from_value(value).ok())
            .collect();
        let skipped = total - entries.len();
        entries.sort_by_key(Entry::at_ms);
        let mut history = Self { path, entries };
        history.prune(now_ms);
        (history, (skipped > 0).then_some(Recovery::Skipped(skipped)))
    }

    fn empty(path: PathBuf) -> Self {
        Self {
            path,
            entries: Vec::new(),
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

    /// Writes a temporary file and renames it over the old one.
    pub fn save(&self) -> io::Result<()> {
        if let Some(directory) = self.path.parent() {
            fs::create_dir_all(directory)?;
        }
        let file = File {
            version: VERSION,
            entries: &self.entries,
        };
        let text = serde_json::to_string(&file).map_err(io::Error::other)?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, text)?;
        fs::rename(&temporary, &self.path)
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
            Entry::Result { .. } => None,
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
