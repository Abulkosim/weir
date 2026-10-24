//! Outage tracking for the opt-in watch: tiny checks once a minute plus OS path changes.

/// Two failed checks in a row start an outage, dated from the first failure.
pub const FAILURES_TO_START: u32 = 2;
/// A gap this long between checks means the computer slept; nothing is known about it.
pub const SLEEP_GAP_MS: u64 = 5 * 60 * 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Lost { since_ms: u64 },
    Back { start_ms: u64, end_ms: u64 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Watch {
    failures: u32,
    first_failure_ms: Option<u64>,
    outage_since_ms: Option<u64>,
    last_seen_ms: Option<u64>,
}

impl Watch {
    pub fn outage_since_ms(&self) -> Option<u64> {
        self.outage_since_ms
    }

    /// Closes an outage across a sleep at the last moment it was observed.
    fn after_gap(&mut self, at_ms: u64) -> Option<Event> {
        let last = self.last_seen_ms.replace(at_ms)?;
        if at_ms.saturating_sub(last) < SLEEP_GAP_MS {
            return None;
        }
        self.failures = 0;
        self.first_failure_ms = None;
        self.outage_since_ms.take().map(|start_ms| Event::Back {
            start_ms,
            end_ms: last.max(start_ms),
        })
    }

    pub fn on_check(&mut self, ok: bool, at_ms: u64) -> Vec<Event> {
        let mut events: Vec<Event> = self.after_gap(at_ms).into_iter().collect();
        if ok {
            self.failures = 0;
            self.first_failure_ms = None;
            if let Some(start_ms) = self.outage_since_ms.take() {
                events.push(Event::Back {
                    start_ms,
                    end_ms: at_ms,
                });
            }
            return events;
        }
        self.failures += 1;
        let first = *self.first_failure_ms.get_or_insert(at_ms);
        if self.outage_since_ms.is_none() && self.failures >= FAILURES_TO_START {
            self.outage_since_ms = Some(first);
            events.push(Event::Lost { since_ms: first });
        }
        events
    }

    /// The OS lost every network path: that is an outage at once, no second check needed.
    pub fn on_offline(&mut self, at_ms: u64) -> Vec<Event> {
        let mut events: Vec<Event> = self.after_gap(at_ms).into_iter().collect();
        if self.outage_since_ms.is_none() {
            let since_ms = self.first_failure_ms.unwrap_or(at_ms);
            self.outage_since_ms = Some(since_ms);
            events.push(Event::Lost { since_ms });
        }
        events
    }

    /// Watching was turned off: forget everything without reporting.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: u64 = 60_000;

    #[test]
    fn one_failed_check_is_not_an_outage() {
        let mut watch = Watch::default();
        assert!(watch.on_check(true, 0).is_empty());
        assert!(watch.on_check(false, MINUTE).is_empty());
        assert!(watch.on_check(true, 2 * MINUTE).is_empty());
        assert_eq!(watch.outage_since_ms(), None);
    }

    #[test]
    fn two_failures_start_an_outage_dated_from_the_first() {
        let mut watch = Watch::default();
        watch.on_check(true, 0);
        assert!(watch.on_check(false, MINUTE).is_empty());
        assert_eq!(
            watch.on_check(false, 2 * MINUTE),
            [Event::Lost { since_ms: MINUTE }]
        );
        assert!(watch.on_check(false, 3 * MINUTE).is_empty());
        assert_eq!(
            watch.on_check(true, 5 * MINUTE),
            [Event::Back {
                start_ms: MINUTE,
                end_ms: 5 * MINUTE
            }]
        );
        assert_eq!(watch.outage_since_ms(), None);
    }

    #[test]
    fn losing_every_path_is_an_outage_at_once() {
        let mut watch = Watch::default();
        watch.on_check(true, 0);
        assert_eq!(watch.on_offline(30_000), [Event::Lost { since_ms: 30_000 }]);
        assert!(watch.on_offline(40_000).is_empty());
        assert!(watch.on_check(false, MINUTE).is_empty());
        assert_eq!(
            watch.on_check(true, 2 * MINUTE),
            [Event::Back {
                start_ms: 30_000,
                end_ms: 2 * MINUTE
            }]
        );
    }

    #[test]
    fn a_sleep_gap_closes_an_outage_where_it_was_last_seen() {
        let mut watch = Watch::default();
        watch.on_check(false, 0);
        watch.on_check(false, MINUTE);
        let after_sleep = 3 * 60 * MINUTE;
        assert_eq!(
            watch.on_check(true, after_sleep),
            [Event::Back {
                start_ms: 0,
                end_ms: MINUTE
            }]
        );
        let mut quiet = Watch::default();
        quiet.on_check(false, 0);
        assert!(
            quiet.on_check(false, after_sleep).is_empty(),
            "a failure before sleep and one after are not two in a row"
        );
    }

    #[test]
    fn flapping_link_counts_one_outage() {
        let mut watch = Watch::default();
        let mut events = Vec::new();
        for (minute, ok) in [true, false, false, false, true, false, true]
            .iter()
            .enumerate()
        {
            events.extend(watch.on_check(*ok, minute as u64 * MINUTE));
        }
        assert_eq!(
            events,
            [
                Event::Lost { since_ms: MINUTE },
                Event::Back {
                    start_ms: MINUTE,
                    end_ms: 4 * MINUTE
                }
            ]
        );
    }

    #[test]
    fn reset_forgets_an_open_outage() {
        let mut watch = Watch::default();
        watch.on_offline(0);
        watch.reset();
        assert_eq!(watch.outage_since_ms(), None);
        assert!(watch.on_check(true, MINUTE).is_empty());
    }
}
