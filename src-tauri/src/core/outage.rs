//! Outage tracking for the opt-in watch: tiny checks once a minute plus OS path changes,
//! and where each failed check broke.

use serde::{Deserialize, Serialize};

/// Two failed checks in a row start an outage, dated from the first failure.
pub const FAILURES_TO_START: u32 = 2;
/// A gap this long between checks means the computer slept; nothing is known about it.
pub const SLEEP_GAP_MS: u64 = 5 * 60 * 1000;

/// Where one failed check broke.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    /// The OS had no network path: Wi-Fi off or cable out.
    NoNetwork,
    /// The router did not answer.
    Router,
    /// The router or its DNS server answered; the internet did not.
    PastRouter,
    Unknown,
}

/// What the probes saw. `router: None` means there was no usable gateway
/// (none, IPv6-only, a tunnel or VPN) or its probe ran out of time.
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

/// How many checks of one outage broke where. Only counts are stored.
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
    /// More than half the checks.
    Mostly {
        cause: Cause,
        count: u32,
        total: u32,
    },
    /// The only known cause, in at most half the checks; the rest were unclear.
    Partly {
        cause: Cause,
        count: u32,
        total: u32,
    },
    Mixed,
}

const KNOWN: [Cause; 3] = [Cause::Router, Cause::PastRouter, Cause::NoNetwork];

fn phrase(cause: Cause) -> &'static str {
    match cause {
        Cause::Router => "at your router",
        Cause::PastRouter => "past your router",
        Cause::NoNetwork => "no network connection",
        Cause::Unknown => "unclear",
    }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

impl Causes {
    pub fn add(&mut self, cause: Cause) {
        let count = match cause {
            Cause::NoNetwork => &mut self.no_network,
            Cause::Router => &mut self.router,
            Cause::PastRouter => &mut self.past_router,
            Cause::Unknown => &mut self.unknown,
        };
        *count = count.saturating_add(1);
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

    /// Router silence that could not be confirmed proves nothing about the router.
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
            None
        } else if count == total {
            Some(Verdict::All(cause))
        } else if u64::from(count) * 2 > u64::from(total) {
            Some(Verdict::Mostly {
                cause,
                count,
                total,
            })
        } else if known == 1 {
            Some(Verdict::Partly {
                cause,
                count,
                total,
            })
        } else {
            Some(Verdict::Mixed)
        }
    }

    /// One plain sentence for History and `weir history`.
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
                "{} in {count} of {total} checks. The rest were unclear.",
                capitalized(phrase(cause))
            ),
            Verdict::Mixed => {
                let total = self.total();
                let mut parts = Vec::new();
                if self.router > 0 {
                    parts.push((self.router, "at your router"));
                }
                if self.past_router > 0 {
                    parts.push((
                        self.past_router,
                        if self.router > 0 {
                            "past it"
                        } else {
                            "past your router"
                        },
                    ));
                }
                if self.no_network > 0 {
                    parts.push((self.no_network, "with no network"));
                }
                if self.unknown > 0 {
                    parts.push((self.unknown, "unclear"));
                }
                let text: Vec<String> = parts
                    .iter()
                    .enumerate()
                    .map(|(index, (count, words))| {
                        if index == 0 {
                            format!("{count} of {total} {words}")
                        } else {
                            format!("{count} {words}")
                        }
                    })
                    .collect();
                format!("Checks disagreed: {}.", text.join(", "))
            }
        })
    }

    /// The short extra sentence for the "Connection back" notification.
    pub fn notice(&self) -> Option<String> {
        match self.verdict()? {
            Verdict::All(Cause::Router) => Some("Your router didn’t answer.".into()),
            Verdict::All(Cause::PastRouter) => Some("The break was past your router.".into()),
            Verdict::All(_) => Some("Your computer had no network connection.".into()),
            Verdict::Mostly { cause, .. } => Some(format!("Mostly {}.", phrase(cause))),
            Verdict::Partly { .. } | Verdict::Mixed => None,
        }
    }

    /// Where it broke, in a few words for the provider report.
    pub fn place(&self) -> Option<String> {
        Some(match self.verdict()? {
            Verdict::All(cause) => place(cause).into(),
            Verdict::Mostly { cause, .. } => format!("mostly {}", place(cause)),
            Verdict::Partly { cause, .. } => format!("partly {}", place(cause)),
            Verdict::Mixed => "checks disagreed".into(),
        })
    }

    /// Every non-zero count, for the CSV export.
    pub fn tally(&self) -> Option<String> {
        let parts: Vec<String> = [
            (self.router, "at your router"),
            (self.past_router, "past your router"),
            (self.no_network, "with no network"),
            (self.unknown, "unclear"),
        ]
        .iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, words)| format!("{count} {words}"))
        .collect();
        (!parts.is_empty()).then(|| format!("checks: {}", parts.join(", ")))
    }

    /// The dominant cause for `weir history`; the counts carry the detail.
    pub fn key(&self) -> Option<&'static str> {
        Some(match self.verdict()? {
            Verdict::All(cause) | Verdict::Mostly { cause, .. } | Verdict::Partly { cause, .. } => {
                match cause {
                    Cause::Router => "router",
                    Cause::PastRouter => "past_router",
                    Cause::NoNetwork | Cause::Unknown => "no_network",
                }
            }
            Verdict::Mixed => "mixed",
        })
    }
}

/// The report's words for one cause.
pub fn place(cause: Cause) -> &'static str {
    match cause {
        Cause::NoNetwork => "no network on this computer",
        other => phrase(other),
    }
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

/// One observation of the watch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    /// The internet answered. `router_now` is the router re-check, when one was due.
    Up {
        router_now: Option<bool>,
    },
    Down(Cause),
    /// The OS reports no path: from the minute tick, or from a path update. Path
    /// updates come in bursts, so they count only as the first sign of an outage.
    Offline {
        from_path: bool,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Watch {
    failures: u32,
    first_failure_ms: Option<u64>,
    outage_since_ms: Option<u64>,
    last_seen_ms: Option<u64>,
    causes: Causes,
    router_unverifiable: bool,
}

impl Watch {
    pub fn outage_since_ms(&self) -> Option<u64> {
        self.outage_since_ms
    }

    /// Router blame stands only if the router answers once the internet is back on
    /// the same network, so the next good check asks it.
    pub fn router_recheck_due(&self) -> bool {
        self.outage_since_ms.is_some() && self.causes.router > 0 && !self.router_unverifiable
    }

    /// The network changed: the router now may not be the one that was silent.
    pub fn network_changed(&mut self) {
        if self.outage_since_ms.is_some() || !self.causes.is_empty() {
            self.router_unverifiable = true;
        }
    }

    fn forget_failures(&mut self) {
        self.failures = 0;
        self.first_failure_ms = None;
        self.causes = Causes::default();
        self.router_unverifiable = false;
    }

    /// Closes an outage across a sleep at the last moment it was observed. Nothing
    /// can be re-checked across a sleep, so router blame goes.
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
        let at_ms = self.last_seen_ms.map_or(at_ms, |last| at_ms.max(last));
        let mut events: Vec<Event> = self.after_gap(at_ms).into_iter().collect();
        match check {
            Check::Up { router_now } => self.on_up(router_now, at_ms, &mut events),
            Check::Down(cause) => self.on_down(cause, at_ms, &mut events),
            Check::Offline { from_path } => self.on_offline(from_path, at_ms, &mut events),
        }
        events
    }

    fn on_up(&mut self, router_now: Option<bool>, at_ms: u64, events: &mut Vec<Event>) {
        let mut causes = self.causes;
        if self.router_unverifiable || router_now != Some(true) {
            causes.demote_router();
        }
        self.forget_failures();
        if let Some(start_ms) = self.outage_since_ms.take() {
            events.push(Event::Back {
                start_ms,
                end_ms: at_ms,
                causes,
            });
        }
    }

    fn on_down(&mut self, cause: Cause, at_ms: u64, events: &mut Vec<Event>) {
        self.failures += 1;
        self.causes.add(cause);
        let first = *self.first_failure_ms.get_or_insert(at_ms);
        if self.outage_since_ms.is_none() && self.failures >= FAILURES_TO_START {
            self.outage_since_ms = Some(first);
            events.push(Event::Lost { since_ms: first });
        }
    }

    /// The OS lost every network path: that is an outage at once, no second check needed.
    fn on_offline(&mut self, from_path: bool, at_ms: u64, events: &mut Vec<Event>) {
        if !from_path || self.causes.is_empty() {
            self.causes.add(Cause::NoNetwork);
        }
        if self.outage_since_ms.is_none() {
            let since_ms = self.first_failure_ms.unwrap_or(at_ms);
            self.outage_since_ms = Some(since_ms);
            events.push(Event::Lost { since_ms });
        }
    }

    /// Watching was turned off: forget everything without reporting.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests;
