//! What the OS says about the current connection: online, and whether data costs money.

use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cost {
    #[default]
    Unknown,
    Unrestricted,
    Fixed,
    Variable,
}

/// Raw path facts from macOS Network.framework or Windows connection profiles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Path {
    pub online: bool,
    /// macOS: cellular or Personal Hotspot.
    pub expensive: bool,
    /// macOS: Low Data Mode.
    pub constrained: bool,
    /// Windows: connection cost type.
    pub cost: Option<Cost>,
    pub roaming: bool,
    pub over_limit: bool,
    pub wifi: bool,
    pub wired: bool,
    pub cellular: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Metered {
    Hotspot,
    LowData,
    Metered,
    Roaming,
    OverLimit,
}

impl Path {
    /// Why a full test should ask first, if it should.
    pub fn metered(&self) -> Option<Metered> {
        if self.over_limit {
            Some(Metered::OverLimit)
        } else if self.roaming {
            Some(Metered::Roaming)
        } else if self.constrained {
            Some(Metered::LowData)
        } else if self.expensive {
            Some(Metered::Hotspot)
        } else {
            match self.cost {
                Some(Cost::Fixed | Cost::Variable | Cost::Unknown) => Some(Metered::Metered),
                Some(Cost::Unrestricted) | None => None,
            }
        }
    }

    /// Facts that, when they change mid-test, mean the result would mix two networks.
    pub fn identity(&self) -> (bool, bool, bool, bool, bool) {
        (
            self.online,
            self.wifi,
            self.wired,
            self.cellular,
            self.expensive,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Network {
    pub online: bool,
    pub metered: Option<Metered>,
    /// When the connection went offline, if it is.
    pub offline_since_ms: Option<u64>,
}

impl Default for Network {
    fn default() -> Self {
        Self {
            online: true,
            metered: None,
            offline_since_ms: None,
        }
    }
}

impl Network {
    pub fn next(&self, path: &Path, now_ms: u64) -> Self {
        Self {
            online: path.online,
            metered: path.metered(),
            offline_since_ms: match (path.online, self.offline_since_ms) {
                (true, _) => None,
                (false, Some(since)) => Some(since),
                (false, None) => Some(now_ms),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path() -> Path {
        Path {
            online: true,
            wifi: true,
            ..Path::default()
        }
    }

    #[test]
    fn plain_wifi_is_not_metered() {
        assert_eq!(path().metered(), None);
        let windows = Path {
            cost: Some(Cost::Unrestricted),
            ..path()
        };
        assert_eq!(windows.metered(), None);
    }

    #[test]
    fn every_costly_signal_asks_first() {
        let cases = [
            (
                Path {
                    expensive: true,
                    ..path()
                },
                Metered::Hotspot,
            ),
            (
                Path {
                    constrained: true,
                    expensive: true,
                    ..path()
                },
                Metered::LowData,
            ),
            (
                Path {
                    cost: Some(Cost::Fixed),
                    ..path()
                },
                Metered::Metered,
            ),
            (
                Path {
                    cost: Some(Cost::Variable),
                    ..path()
                },
                Metered::Metered,
            ),
            (
                Path {
                    cost: Some(Cost::Unknown),
                    ..path()
                },
                Metered::Metered,
            ),
            (
                Path {
                    roaming: true,
                    cost: Some(Cost::Variable),
                    ..path()
                },
                Metered::Roaming,
            ),
            (
                Path {
                    over_limit: true,
                    roaming: true,
                    ..path()
                },
                Metered::OverLimit,
            ),
        ];
        for (path, expected) in cases {
            assert_eq!(path.metered(), Some(expected), "{path:?}");
        }
    }

    #[test]
    fn offline_since_is_kept_until_back_online() {
        let start = Network::default();
        let offline = Path {
            online: false,
            ..path()
        };
        let first = start.next(&offline, 1_000);
        assert_eq!(first.offline_since_ms, Some(1_000));
        let still = first.next(&offline, 9_000);
        assert_eq!(still.offline_since_ms, Some(1_000));
        let back = still.next(&path(), 12_000);
        assert!(back.online);
        assert_eq!(back.offline_since_ms, None);
    }

    #[test]
    fn identity_ignores_low_data_toggles_but_not_interface_changes() {
        let wifi = path();
        let low = Path {
            constrained: true,
            ..wifi
        };
        let wired = Path {
            wifi: false,
            wired: true,
            ..wifi
        };
        assert_eq!(wifi.identity(), low.identity());
        assert_ne!(wifi.identity(), wired.identity());
    }
}
