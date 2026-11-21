use jiff::{
    tz::{Offset, TimeZone},
    Timestamp, ZonedDateTime,
};
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
    Fixed(Offset),
}

impl Zone {
    /// A fixed zone `hours` east of UTC.
    pub fn hours(hours: i8) -> Self {
        Self::Fixed(Offset::constant(hours))
    }

    pub fn at(self, ms: u64) -> ZonedDateTime {
        let time = i64::try_from(ms)
            .ok()
            .and_then(|ms| Timestamp::from_millisecond(ms).ok())
            .unwrap_or(Timestamp::UNIX_EPOCH);
        match self {
            Self::Local => time.to_zoned_datetime(TimeZone::system()),
            Self::Fixed(offset) => time.to_zoned_datetime(TimeZone::fixed(offset)),
        }
    }
}

/// "2026-10-22T14:02:00+05:00": seconds and the offset that applied on that date.
pub fn rfc3339(zone: Zone, ms: u64) -> String {
    zone.at(ms).strftime("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

/// Local "14:02".
pub fn clock(ms: u64) -> String {
    Zone::Local.at(ms).strftime("%H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_zone_shifts_the_clock() {
        let zone = Zone::hours(5);
        assert_eq!(zone.at(0).strftime("%H:%M").to_string(), "05:00");
        assert_eq!(rfc3339(zone, 0), "1970-01-01T05:00:00+05:00");
        assert_eq!(rfc3339(Zone::hours(0), 1_500), "1970-01-01T00:00:01+00:00");
    }

    #[test]
    fn times_past_the_calendar_fall_back_to_the_epoch() {
        assert_eq!(
            rfc3339(Zone::hours(0), u64::MAX),
            "1970-01-01T00:00:00+00:00"
        );
    }
}
