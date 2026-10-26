//! Settings in their own versioned file. Each field loads on its own, so one bad
//! value never resets the rest.

use super::plan::{self, Plan};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub const VERSION: u64 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub theme: Theme,
    /// macOS: show the last download speed beside the tray icon.
    pub tray_speed: bool,
    pub autostart: bool,
    /// Check GitHub for a new release once a day. Never installs on its own.
    pub auto_check_updates: bool,
    /// A tiny reachability check every minute to record outages. Off by default.
    pub watch_outages: bool,
    /// Notify when a watched connection drops and when it returns.
    pub notify_outages: bool,
    /// The download speed the provider sells, in Mbps.
    pub plan_download_mbps: Option<u32>,
    /// The upload speed the provider sells, in Mbps.
    pub plan_upload_mbps: Option<u32>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            tray_speed: false,
            autostart: false,
            auto_check_updates: true,
            watch_outages: false,
            notify_outages: true,
            plan_download_mbps: None,
            plan_upload_mbps: None,
        }
    }
}

fn field<T: DeserializeOwned>(
    map: &Map<String, Value>,
    key: &str,
    fallback: T,
    label: &str,
    notes: &mut Vec<String>,
) -> T {
    match map.get(key) {
        None => fallback,
        Some(value) => serde_json::from_value(value.clone()).unwrap_or_else(|_| {
            notes.push(format!("{label} was reset."));
            fallback
        }),
    }
}

impl Settings {
    fn recover(map: &Map<String, Value>, notes: &mut Vec<String>) -> Self {
        let defaults = Self::default();
        Self {
            theme: field(map, "theme", defaults.theme, "Appearance", notes),
            tray_speed: field(
                map,
                "traySpeed",
                defaults.tray_speed,
                "Speed next to the icon",
                notes,
            ),
            autostart: field(
                map,
                "autostart",
                defaults.autostart,
                "Launch at login",
                notes,
            ),
            auto_check_updates: field(
                map,
                "autoCheckUpdates",
                defaults.auto_check_updates,
                "Update checks",
                notes,
            ),
            watch_outages: field(
                map,
                "watchOutages",
                defaults.watch_outages,
                "Watch for outages",
                notes,
            ),
            notify_outages: field(
                map,
                "notifyOutages",
                defaults.notify_outages,
                "Outage notifications",
                notes,
            ),
            plan_download_mbps: plan::valid(field(
                map,
                "planDownloadMbps",
                defaults.plan_download_mbps,
                "Plan download speed",
                notes,
            )),
            plan_upload_mbps: plan::valid(field(
                map,
                "planUploadMbps",
                defaults.plan_upload_mbps,
                "Plan upload speed",
                notes,
            )),
        }
    }

    pub fn plan(&self) -> Plan {
        Plan {
            download_mbps: plan::valid(self.plan_download_mbps),
            upload_mbps: plan::valid(self.plan_upload_mbps),
        }
    }
}

#[derive(Debug)]
pub struct Store {
    path: PathBuf,
    /// False when the file is from a newer Weir: never overwrite it.
    writable: bool,
}

#[derive(Serialize)]
struct File<'a> {
    version: u64,
    settings: &'a Settings,
}

fn aside(path: &Path, now_ms: u64) -> PathBuf {
    path.with_file_name(format!("settings.unreadable-{now_ms}.json"))
}

impl Store {
    /// Loads settings and any notes about what had to be reset.
    pub fn load(path: PathBuf, now_ms: u64) -> (Self, Settings, Vec<String>) {
        let mut notes = Vec::new();
        let Ok(text) = fs::read_to_string(&path) else {
            return (
                Self {
                    path,
                    writable: true,
                },
                Settings::default(),
                notes,
            );
        };
        let root = serde_json::from_str::<Value>(&text).ok();
        let Some(Value::Object(root)) = root else {
            let backup = aside(&path, now_ms);
            let moved = fs::rename(&path, &backup).is_ok();
            notes.push(if moved {
                format!(
                    "Settings couldn’t be read and were reset. The old file was kept as {}.",
                    backup
                        .file_name()
                        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
                )
            } else {
                "Settings couldn’t be read and were reset.".into()
            });
            return (
                Self {
                    path,
                    writable: moved,
                },
                Settings::default(),
                notes,
            );
        };
        if root
            .get("version")
            .and_then(Value::as_u64)
            .is_some_and(|version| version > VERSION)
        {
            notes.push(
                "These settings are from a newer version of Weir. Changes here won’t be saved."
                    .into(),
            );
            return (
                Self {
                    path,
                    writable: false,
                },
                Settings::default(),
                notes,
            );
        }
        let settings = match root.get("settings") {
            Some(Value::Object(map)) => Settings::recover(map, &mut notes),
            _ => Settings::default(),
        };
        (
            Self {
                path,
                writable: true,
            },
            settings,
            notes,
        )
    }

    pub fn save(&self, settings: &Settings) -> io::Result<()> {
        if !self.writable {
            return Err(io::Error::other("settings file is from a newer version"));
        }
        if let Some(directory) = self.path.parent() {
            fs::create_dir_all(directory)?;
        }
        let text = serde_json::to_string_pretty(&File {
            version: VERSION,
            settings,
        })
        .map_err(io::Error::other)?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, text)?;
        fs::rename(&temporary, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with(text: &str) -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        fs::write(&path, text).unwrap();
        (directory, path)
    }

    #[test]
    fn missing_file_gives_defaults_without_notes() {
        let directory = tempfile::tempdir().unwrap();
        let (_, settings, notes) = Store::load(directory.path().join("settings.json"), 1);
        assert_eq!(settings, Settings::default());
        assert!(notes.is_empty());
    }

    #[test]
    fn saves_and_loads_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let (store, _, _) = Store::load(path.clone(), 1);
        let settings = Settings {
            theme: Theme::Dark,
            tray_speed: true,
            autostart: true,
            auto_check_updates: false,
            watch_outages: true,
            notify_outages: false,
            plan_download_mbps: Some(500),
            plan_upload_mbps: Some(50),
        };
        store.save(&settings).unwrap();
        let (_, loaded, notes) = Store::load(path, 2);
        assert_eq!(loaded, settings);
        assert!(notes.is_empty());
    }

    #[test]
    fn one_bad_field_keeps_the_others() {
        let (_directory, path) = store_with(
            r#"{"version":1,"settings":{"theme":"purple","traySpeed":true,"autostart":"yes","future":1}}"#,
        );
        let (_, settings, notes) = Store::load(path, 1);
        assert_eq!(settings.theme, Theme::System);
        assert!(settings.tray_speed);
        assert!(!settings.autostart);
        assert_eq!(
            notes,
            ["Appearance was reset.", "Launch at login was reset."]
        );
    }

    #[test]
    fn plan_speeds_out_of_range_are_dropped() {
        let (_directory, path) = store_with(
            r#"{"version":1,"settings":{"planDownloadMbps":0,"planUploadMbps":"fast"}}"#,
        );
        let (_, settings, notes) = Store::load(path, 1);
        assert_eq!(settings.plan(), Plan::default());
        assert_eq!(notes, ["Plan upload speed was reset."]);
    }

    #[test]
    fn unreadable_file_is_kept_aside() {
        let (_directory, path) = store_with("{ nope");
        let (store, settings, notes) = Store::load(path.clone(), 77);
        assert_eq!(settings, Settings::default());
        assert!(notes[0].contains("settings.unreadable-77.json"));
        assert_eq!(
            fs::read_to_string(path.with_file_name("settings.unreadable-77.json")).unwrap(),
            "{ nope"
        );
        store.save(&settings).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn newer_file_is_never_overwritten() {
        let text = r#"{"version":9,"settings":{"theme":"dark"}}"#;
        let (_directory, path) = store_with(text);
        let (store, settings, notes) = Store::load(path.clone(), 1);
        assert_eq!(settings, Settings::default());
        assert_eq!(notes.len(), 1);
        assert!(store.save(&Settings::default()).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), text);
    }
}
