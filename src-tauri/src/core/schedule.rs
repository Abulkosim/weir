//! Opt-in scheduled full tests. A test starts only after the connection has been
//! suitable (online, not metered, not in Low Power Mode, nothing else running) for a
//! couple of minutes, and only when no test, by hand or scheduled, ran within the interval.

use super::settings::Schedule;

/// Suitable this long first, so a test never fires straight after wake, login or a network change.
pub const SETTLE_MS: u64 = 2 * 60_000;
/// A longer gap between checks means the computer slept; settling starts again.
const SLEEP_GAP_MS: u64 = 3 * 60_000;
const HOUR_MS: u64 = 3_600_000;

impl Schedule {
    pub fn interval_ms(self) -> Option<u64> {
        match self {
            Self::Off => None,
            Self::Every6Hours => Some(6 * HOUR_MS),
            Self::Every12Hours => Some(12 * HOUR_MS),
            Self::Daily => Some(24 * HOUR_MS),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Scheduler {
    suitable_since_ms: Option<u64>,
    last_tick_ms: Option<u64>,
    last_started_ms: Option<u64>,
}

impl Scheduler {
    /// One check, about once a minute. `last_test_ms` is the newest test in history.
    /// True when a scheduled test should start now.
    pub fn tick(
        &mut self,
        schedule: Schedule,
        suitable: bool,
        last_test_ms: Option<u64>,
        now_ms: u64,
    ) -> bool {
        let slept = self
            .last_tick_ms
            .replace(now_ms)
            .is_some_and(|last| now_ms.saturating_sub(last) > SLEEP_GAP_MS);
        let Some(interval) = schedule.interval_ms() else {
            self.suitable_since_ms = None;
            return false;
        };
        if !suitable || slept {
            self.suitable_since_ms = suitable.then_some(now_ms);
            return false;
        }
        let since = *self.suitable_since_ms.get_or_insert(now_ms);
        let elapsed = |at: Option<u64>| at.is_none_or(|at| now_ms.saturating_sub(at) >= interval);
        if now_ms.saturating_sub(since) < SETTLE_MS
            || !elapsed(last_test_ms)
            || !elapsed(self.last_started_ms)
        {
            return false;
        }
        self.last_started_ms = Some(now_ms);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: u64 = 60_000;
    const DAY: u64 = 24 * HOUR_MS;

    fn run(
        scheduler: &mut Scheduler,
        schedule: Schedule,
        minutes: std::ops::Range<u64>,
        last: Option<u64>,
    ) -> Vec<u64> {
        minutes
            .filter(|minute| scheduler.tick(schedule, true, last, DAY + minute * MINUTE))
            .collect()
    }

    #[test]
    fn off_never_runs() {
        let mut scheduler = Scheduler::default();
        assert!(run(&mut scheduler, Schedule::Off, 0..600, None).is_empty());
    }

    #[test]
    fn waits_for_the_connection_to_settle_then_runs_once() {
        let mut scheduler = Scheduler::default();
        assert_eq!(
            run(&mut scheduler, Schedule::Every6Hours, 0..300, None),
            [2]
        );
    }

    #[test]
    fn a_recent_test_by_hand_pushes_the_next_one_back() {
        let mut scheduler = Scheduler::default();
        let by_hand = DAY - 5 * HOUR_MS;
        assert_eq!(
            run(&mut scheduler, Schedule::Every6Hours, 0..120, Some(by_hand)),
            [60]
        );
    }

    #[test]
    fn a_cancelled_scheduled_test_is_not_retried_at_once() {
        let mut scheduler = Scheduler::default();
        assert_eq!(run(&mut scheduler, Schedule::Daily, 0..10, None), [2]);
        assert!(
            run(&mut scheduler, Schedule::Daily, 10..600, None).is_empty(),
            "history has no entry, but the scheduler remembers starting"
        );
    }

    #[test]
    fn unsuitable_moments_reset_the_settling() {
        let mut scheduler = Scheduler::default();
        assert!(!scheduler.tick(Schedule::Daily, true, None, DAY));
        assert!(!scheduler.tick(Schedule::Daily, true, None, DAY + MINUTE));
        assert!(!scheduler.tick(Schedule::Daily, false, None, DAY + 2 * MINUTE));
        assert!(!scheduler.tick(Schedule::Daily, true, None, DAY + 3 * MINUTE));
        assert!(!scheduler.tick(Schedule::Daily, true, None, DAY + 4 * MINUTE));
        assert!(scheduler.tick(Schedule::Daily, true, None, DAY + 5 * MINUTE));
    }

    #[test]
    fn waking_from_sleep_settles_again() {
        let mut scheduler = Scheduler::default();
        assert!(!scheduler.tick(Schedule::Daily, true, Some(0), DAY - HOUR_MS));
        let wake = DAY + 8 * HOUR_MS;
        assert!(!scheduler.tick(Schedule::Daily, true, Some(0), wake));
        assert!(!scheduler.tick(Schedule::Daily, true, Some(0), wake + MINUTE));
        assert!(scheduler.tick(Schedule::Daily, true, Some(0), wake + 2 * MINUTE));
    }
}
