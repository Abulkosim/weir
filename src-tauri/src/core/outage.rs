//! Outage tracking for the opt-in watch: tiny checks once a minute plus OS path changes.
//! Each failed check is classified by where the connection broke; only counts are kept.

use serde::{Deserialize, Serialize};

/// Two failed checks in a row start an outage, dated from the first failure.
pub const FAILURES_TO_START: u32 = 2;
/// A gap this long between checks means the computer slept; nothing is known about it.
pub const SLEEP_GAP_MS: u64 = 5 * 60 * 1000;

/// Where one failed check broke.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    /// The OS reported no network path: Wi-Fi off or cable out.
    NoNetwork,
    /// The router did not answer.
    Router,
    /// The router or the DNS server answered; the internet past them did not.
    PastRouter,
    /// No usable router to ask, or the probes ran out of time.
    Unknown,
}

/// What the probes saw. `router: None` means there was no usable gateway to ask
/// (none, IPv6 only, a VPN tunnel) or the probe ran out of time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Signals {
    pub router: Option<bool>,
    pub dns: Option<bool>,
}

pub fn classify(signals: Signals) -> Cause {
    match signals {
        Signals { router: None, .. } => Cause::Unknown,
        Signals {
            router: Some(true), ..
        }
        | Signals {
            dns: Some(true), ..
        } => Cause::PastRouter,
        Signals {
            router: Some(false),
            ..
        } => Cause::Router,
    }
}

/// How many checks of one outage broke where. Stored in History.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Causes {
    pub no_network: u32,
    pub router: u32,
    pub past_router: u32,
    pub unknown: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    All(Cause),
    Mostly {
        cause: Cause,
        count: u32,
        total: u32,
    },
    Partly {
        cause: Cause,
        count: u32,
        total: u32,
    },
    Mixed,
}

const KNOWN: [Cause; 3] = [Cause::Router, Cause::PastRouter, Cause::NoNetwork];

impl Causes {
    pub fn add(&mut self, cause: Cause) {
        let slot = match cause {
            Cause::NoNetwork => &mut self.no_network,
            Cause::Router => &mut self.router,
            Cause::PastRouter => &mut self.past_router,
            Cause::Unknown => &mut self.unknown,
        };
        *slot = slot.saturating_add(1);
    }

    pub fn count(&self, cause: Cause) -> u32 {
        match cause {
            Cause::NoNetwork => self.no_network,
            Cause::Router => self.router,
            Cause::PastRouter => self.past_router,
            Cause::Unknown => self.unknown,
        }
    }

    pub fn total(&self) -> u32 {
        self.no_network
            .saturating_add(self.router)
            .saturating_add(self.past_router)
            .saturating_add(self.unknown)
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }

    /// Router blame that could not be confirmed becomes unclear.
    fn demote_router(&mut self) {
        self.unknown = self.unknown.saturating_add(self.router);
        self.router = 0;
    }

    pub fn verdict(&self) -> Option<Verdict> {
        let total = self.total();
        let (cause, count) = KNOWN.iter().map(|cause| (*cause, self.count(*cause))).fold(
            (Cause::Unknown, 0),
            |best, next| {
                if next.1 > best.1 {
                    next
                } else {
                    best
                }
            },
        );
        let known = KNOWN.iter().filter(|cause| self.count(**cause) > 0).count();
        if count == 0 {
            return None;
        }
        Some(if count == total {
            Verdict::All(cause)
        } else if u64::from(count) * 2 > u64::from(total) {
            Verdict::Mostly {
                cause,
                count,
                total,
            }
        } else if known == 1 {
            Verdict::Partly {
                cause,
                count,
                total,
            }
        } else {
            Verdict::Mixed
        })
    }

    /// One plain sentence for History and the command line.
    pub fn sentence(&self) -> Option<String> {
        Some(match self.verdict()? {
            Verdict::All(Cause::Router) => {
                "Your router didn’t answer. The problem was at home: the Wi-Fi, a cable or the router."
                    .into()
            }
            Verdict::All(Cause::PastRouter) => {
                "Your router was reachable throughout. The break was past it.".into()
            }
            Verdict::All(_) => "Your computer had no network connection.".into(),
            Verdict::Mostly {
                cause,
                count,
                total,
            } => format!("Mostly {}: {count} of {total} checks.", phrase(cause)),
            Verdict::Partly {
                cause,
                count,
                total,
            } => format!(
                "{} in {count} of {total} checks. {}",
                capitalized(phrase(cause)),
                if total - count == 1 {
                    "The other was unclear."
                } else {
                    "The rest were unclear."
                }
            ),
            Verdict::Mixed => {
                let total = self.total();
                let parts: Vec<String> = [
                    (self.router, "at your router"),
                    (
                        self.past_router,
                        if self.router > 0 {
                            "past it"
                        } else {
                            "past your router"
                        },
                    ),
                    (self.no_network, "with no network"),
                    (self.unknown, "unclear"),
                ]
                .into_iter()
                .filter(|(count, _)| *count > 0)
                .enumerate()
                .map(|(index, (count, label))| {
                    if index == 0 {
                        format!("{count} of {total} {label}")
                    } else {
                        format!("{count} {label}")
                    }
                })
                .collect();
                format!("Checks disagreed: {}.", parts.join(", "))
            }
        })
    }

    /// At most one short sentence for the "Connection back" notification.
    pub fn notice(&self) -> Option<String> {
        match self.verdict()? {
            Verdict::All(Cause::Router) => Some("Your router didn’t answer.".into()),
            Verdict::All(Cause::PastRouter) => Some("The break was past your router.".into()),
            Verdict::All(_) => Some("Your computer had no network connection.".into()),
            Verdict::Mostly { cause, .. } => Some(format!("Mostly {}.", phrase(cause))),
            Verdict::Partly { .. } | Verdict::Mixed => None,
        }
    }

    /// A short place for the provider report, for example "past your router".
    pub fn place(&self) -> Option<String> {
        Some(match self.verdict()? {
            Verdict::All(cause) => place(cause).into(),
            Verdict::Mostly { cause, .. } => format!("mostly {}", place(cause)),
            Verdict::Partly { cause, .. } => format!("partly {}", place(cause)),
            Verdict::Mixed => "checks disagreed".into(),
        })
    }

    /// Every non-zero count, for the CSV notes.
    pub fn tally(&self) -> Option<String> {
        let parts: Vec<String> = [
            (self.router, "at your router"),
            (self.past_router, "past your router"),
            (self.no_network, "with no network"),
            (self.unknown, "unclear"),
        ]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, label)| format!("{count} {label}"))
        .collect();
        (!parts.is_empty()).then(|| format!("checks: {}", parts.join(", ")))
    }

    /// A stable key for scripts: `router`, `past_router`, `no_network` or `mixed`.
    pub fn key(&self) -> Option<&'static str> {
        match self.verdict()? {
            Verdict::All(cause) | Verdict::Mostly { cause, .. } | Verdict::Partly { cause, .. } => {
                Some(key(cause))
            }
            Verdict::Mixed => Some("mixed"),
        }
    }
}

fn phrase(cause: Cause) -> &'static str {
    match cause {
        Cause::Router => "at your router",
        Cause::PastRouter => "past your router",
        Cause::NoNetwork => "no network connection",
        Cause::Unknown => "unclear",
    }
}

pub fn place(cause: Cause) -> &'static str {
    match cause {
        Cause::NoNetwork => "no network on this computer",
        other => phrase(other),
    }
}

fn key(cause: Cause) -> &'static str {
    match cause {
        Cause::Router => "router",
        Cause::PastRouter => "past_router",
        Cause::NoNetwork => "no_network",
        Cause::Unknown => "unknown",
    }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Lost {
        since_ms: u64,
    },
    Back {
        start_ms: u64,
        end_ms: u64,
        causes: Causes,
    },
}

/// One input to the watch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    /// The internet answered. `router_now` is the router re-check, when one was due.
    Up { router_now: Option<bool> },
    /// The internet did not answer, and this is where it broke.
    Down(Cause),
    /// The OS reports no network path: from the minute check, or from a path update,
    /// which can arrive in bursts and is counted only as the first sign of an outage.
    Offline { from_path: bool },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Watch {
    failures: u32,
    first_failure_ms: Option<u64>,
    outage_since_ms: Option<u64>,
    last_seen_ms: Option<u64>,
    causes: Causes,
    /// The network changed since the first failure, so router silence can't be re-checked.
    router_unverifiable: bool,
}

impl Watch {
    pub fn outage_since_ms(&self) -> Option<u64> {
        self.outage_since_ms
    }

    pub fn causes(&self) -> Causes {
        self.causes
    }

    fn forget_failures(&mut self) {
        self.failures = 0;
        self.first_failure_ms = None;
        self.causes = Causes::default();
        self.router_unverifiable = false;
    }

    /// Closes an outage across a sleep at the last moment it was observed. Router
    /// blame can't be re-checked across a sleep, so it becomes unclear.
    fn after_gap(&mut self, at_ms: u64) -> Option<Event> {
        let last = self.last_seen_ms.replace(at_ms)?;
        if at_ms.saturating_sub(last) < SLEEP_GAP_MS {
            return None;
        }
        let mut causes = self.causes;
        causes.demote_router();
        self.forget_failures();
        self.outage_since_ms.take().map(|start_ms| Event::Back {
            start_ms,
            end_ms: last.max(start_ms),
            causes,
        })
    }

    pub fn on(&mut self, check: Check, at_ms: u64) -> Vec<Event> {
        let at_ms = at_ms.max(self.last_seen_ms.unwrap_or(0));
        let mut events: Vec<Event> = self.after_gap(at_ms).into_iter().collect();
        events.extend(match check {
            Check::Up { router_now } => self.on_up(router_now, at_ms),
            Check::Down(cause) => self.on_down(cause, at_ms),
            Check::Offline { from_path } => self.on_offline(from_path, at_ms),
        });
        events
    }

    fn on_up(&mut self, router_now: Option<bool>, at_ms: u64) -> Option<Event> {
        let mut causes = self.causes;
        if self.router_unverifiable || router_now != Some(true) {
            causes.demote_router();
        }
        self.forget_failures();
        self.outage_since_ms.take().map(|start_ms| Event::Back {
            start_ms,
            end_ms: at_ms,
            causes,
        })
    }

    fn on_down(&mut self, cause: Cause, at_ms: u64) -> Option<Event> {
        self.failures += 1;
        self.causes.add(cause);
        let first = *self.first_failure_ms.get_or_insert(at_ms);
        if self.outage_since_ms.is_none() && self.failures >= FAILURES_TO_START {
            self.outage_since_ms = Some(first);
            return Some(Event::Lost { since_ms: first });
        }
        None
    }

    /// The OS lost every network path: that is an outage at once, no second check needed.
    fn on_offline(&mut self, from_path: bool, at_ms: u64) -> Option<Event> {
        if !from_path || self.causes.is_empty() {
            self.causes.add(Cause::NoNetwork);
        }
        if self.outage_since_ms.is_some() {
            return None;
        }
        let since_ms = *self.first_failure_ms.get_or_insert(at_ms);
        self.outage_since_ms = Some(since_ms);
        Some(Event::Lost { since_ms })
    }

    /// A re-check of the router is due once the internet is back: router blame
    /// stands only if the router answers then.
    pub fn router_recheck_due(&self) -> bool {
        self.outage_since_ms.is_some() && self.causes.router > 0 && !self.router_unverifiable
    }

    /// The OS moved to another network: a router re-check would ask a different router.
    pub fn network_changed(&mut self) {
        if self.outage_since_ms.is_some() || !self.causes.is_empty() {
            self.router_unverifiable = true;
        }
    }

    /// Watching was turned off: forget everything without reporting.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests;
