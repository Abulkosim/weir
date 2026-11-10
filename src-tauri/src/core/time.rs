use chrono::{DateTime, FixedOffset, Local};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_millis() as u64)
}

/// Where times are shown: the system's zone, which gives each date its own
/// offset across daylight saving changes, or one fixed offset in tests.
#[derive(Clone, Copy, Debug)]
pub enum Zone {
    Local,
    Fixed(FixedOffset),
}

impl Zone {
    pub fn at(self, ms: u64) -> DateTime<FixedOffset> {
        let time = DateTime::from_timestamp_millis(ms as i64).unwrap_or_default();
        match self {
            Self::Local => time.with_timezone(&Local).fixed_offset(),
            Self::Fixed(offset) => time.with_timezone(&offset),
        }
    }
}

/// Local "14:02".
pub fn clock(ms: u64) -> String {
    Zone::Local.at(ms).format("%H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_zone_shifts_the_clock() {
        let zone = Zone::Fixed(FixedOffset::east_opt(5 * 3600).unwrap());
        assert_eq!(zone.at(0).format("%H:%M").to_string(), "05:00");
    }
}
