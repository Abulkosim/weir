use super::*;

const MINUTE: u64 = 60_000;
const UP: Check = Check::Up { router_now: None };
const PAST: Check = Check::Down(Cause::PastRouter);
const ROUTER: Check = Check::Down(Cause::Router);
const PATH_OFFLINE: Check = Check::Offline { from_path: true };
const TICK_OFFLINE: Check = Check::Offline { from_path: false };

fn causes(no_network: u32, router: u32, past_router: u32, unknown: u32) -> Causes {
    Causes {
        no_network,
        router,
        past_router,
        unknown,
    }
}

fn back(start_ms: u64, end_ms: u64, causes: Causes) -> Event {
    Event::Back {
        start_ms,
        end_ms,
        causes,
    }
}

#[test]
fn classify_blames_the_router_only_when_it_and_its_dns_were_silent() {
    let cases = [
        (None, None, Cause::Unknown),
        (None, Some(true), Cause::Unknown),
        (Some(true), None, Cause::PastRouter),
        (Some(false), Some(true), Cause::PastRouter),
        (Some(false), Some(false), Cause::Router),
        (Some(false), None, Cause::Router),
    ];
    for (router, dns, cause) in cases {
        assert_eq!(
            classify(Signals { router, dns }),
            cause,
            "{router:?} {dns:?}"
        );
    }
}

#[test]
fn one_failed_check_is_not_an_outage() {
    let mut watch = Watch::default();
    assert!(watch.on(UP, 0).is_empty());
    assert!(watch.on(PAST, MINUTE).is_empty());
    assert!(watch.on(UP, 2 * MINUTE).is_empty());
    assert_eq!(watch.outage_since_ms(), None);
    watch.on(ROUTER, 3 * MINUTE);
    watch.on(ROUTER, 4 * MINUTE);
    assert_eq!(
        watch.on(
            Check::Up {
                router_now: Some(true)
            },
            5 * MINUTE
        ),
        [back(3 * MINUTE, 5 * MINUTE, causes(0, 2, 0, 0))],
        "the lone failure before was forgotten"
    );
}

#[test]
fn two_failures_start_an_outage_dated_from_the_first_and_both_count() {
    let mut watch = Watch::default();
    watch.on(UP, 0);
    assert!(watch.on(PAST, MINUTE).is_empty());
    assert_eq!(
        watch.on(PAST, 2 * MINUTE),
        [Event::Lost { since_ms: MINUTE }]
    );
    assert!(watch.on(Check::Down(Cause::Unknown), 3 * MINUTE).is_empty());
    assert_eq!(
        watch.on(UP, 5 * MINUTE),
        [back(MINUTE, 5 * MINUTE, causes(0, 0, 2, 1))]
    );
    assert_eq!(watch.outage_since_ms(), None);
}

#[test]
fn losing_every_path_is_an_outage_at_once_and_a_burst_counts_once() {
    let mut watch = Watch::default();
    watch.on(UP, 0);
    assert_eq!(
        watch.on(PATH_OFFLINE, 30_000),
        [Event::Lost { since_ms: 30_000 }]
    );
    for second in 31..35 {
        assert!(watch.on(PATH_OFFLINE, second * 1_000).is_empty());
    }
    assert!(watch.on(PAST, MINUTE).is_empty());
    assert_eq!(
        watch.on(UP, 2 * MINUTE),
        [back(30_000, 2 * MINUTE, causes(1, 0, 1, 0))]
    );
}

#[test]
fn a_path_event_after_a_failed_check_adds_nothing() {
    let mut watch = Watch::default();
    watch.on(PAST, 0);
    assert_eq!(
        watch.on(PATH_OFFLINE, 10_000),
        [Event::Lost { since_ms: 0 }]
    );
    assert_eq!(watch.on(UP, MINUTE), [back(0, MINUTE, causes(0, 0, 1, 0))]);
}

#[test]
fn every_offline_minute_tick_counts() {
    let mut watch = Watch::default();
    for minute in 0..3 {
        watch.on(TICK_OFFLINE, minute * MINUTE);
        watch.on(PATH_OFFLINE, minute * MINUTE + 1);
    }
    assert_eq!(
        watch.on(UP, 3 * MINUTE),
        [back(0, 3 * MINUTE, causes(3, 0, 0, 0))]
    );
}

#[test]
fn a_sleep_gap_closes_an_outage_where_it_was_last_seen_without_router_blame() {
    let mut watch = Watch::default();
    watch.on(ROUTER, 0);
    watch.on(ROUTER, MINUTE);
    let after_sleep = 3 * 60 * MINUTE;
    assert_eq!(
        watch.on(
            Check::Up {
                router_now: Some(true)
            },
            after_sleep
        ),
        [back(0, MINUTE, causes(0, 0, 0, 2))]
    );
    let mut quiet = Watch::default();
    quiet.on(PAST, 0);
    assert!(
        quiet.on(PAST, after_sleep).is_empty(),
        "a failure before sleep and one after are not two in a row"
    );
    assert_eq!(
        quiet.on(PAST, after_sleep + MINUTE),
        [Event::Lost {
            since_ms: after_sleep
        }]
    );
    assert_eq!(
        quiet.on(UP, after_sleep + 2 * MINUTE),
        [back(
            after_sleep,
            after_sleep + 2 * MINUTE,
            causes(0, 0, 2, 0)
        )],
        "the next outage starts fresh"
    );
}

#[test]
fn router_blame_needs_the_router_to_answer_once_the_internet_is_back() {
    let outage = || {
        let mut watch = Watch::default();
        watch.on(ROUTER, 0);
        watch.on(ROUTER, MINUTE);
        watch.on(PAST, 2 * MINUTE);
        watch
    };
    let mut watch = outage();
    assert!(watch.router_recheck_due());
    assert_eq!(
        watch.on(
            Check::Up {
                router_now: Some(true)
            },
            3 * MINUTE
        ),
        [back(0, 3 * MINUTE, causes(0, 2, 1, 0))]
    );
    assert!(!watch.router_recheck_due());
    for silent in [Some(false), None] {
        let mut watch = outage();
        assert_eq!(
            watch.on(Check::Up { router_now: silent }, 3 * MINUTE),
            [back(0, 3 * MINUTE, causes(0, 0, 1, 2))],
            "{silent:?}"
        );
    }
}

#[test]
fn a_network_change_takes_router_blame_away() {
    let mut watch = Watch::default();
    watch.on(ROUTER, 0);
    watch.on(ROUTER, MINUTE);
    watch.network_changed();
    assert!(!watch.router_recheck_due());
    assert_eq!(
        watch.on(
            Check::Up {
                router_now: Some(true)
            },
            2 * MINUTE
        ),
        [back(0, 2 * MINUTE, causes(0, 0, 0, 2))]
    );
    let mut calm = Watch::default();
    calm.network_changed();
    calm.on(ROUTER, 3 * MINUTE);
    calm.on(ROUTER, 4 * MINUTE);
    assert!(
        calm.router_recheck_due(),
        "a change before the outage proves nothing"
    );
}

#[test]
fn router_recheck_is_due_only_during_an_outage_with_router_blame() {
    let mut watch = Watch::default();
    watch.on(ROUTER, 0);
    assert!(!watch.router_recheck_due(), "no outage yet");
    watch.on(PAST, MINUTE);
    assert!(watch.router_recheck_due());
    let mut past = Watch::default();
    past.on(PAST, 0);
    past.on(PAST, MINUTE);
    assert!(!past.router_recheck_due());
}

#[test]
fn flapping_link_counts_one_outage() {
    let mut watch = Watch::default();
    let mut events = Vec::new();
    for (minute, ok) in [true, false, false, false, true, false, true]
        .iter()
        .enumerate()
    {
        let check = if *ok { UP } else { PAST };
        events.extend(watch.on(check, minute as u64 * MINUTE));
    }
    assert_eq!(
        events,
        [
            Event::Lost { since_ms: MINUTE },
            back(MINUTE, 4 * MINUTE, causes(0, 0, 3, 0))
        ]
    );
}

#[test]
fn reset_forgets_an_open_outage_and_its_counts() {
    let mut watch = Watch::default();
    watch.on(TICK_OFFLINE, 0);
    watch.on(ROUTER, 1);
    watch.reset();
    assert_eq!(watch, Watch::default());
    assert_eq!(watch.outage_since_ms(), None);
    assert!(watch.on(UP, MINUTE).is_empty());
}

#[test]
fn time_never_runs_backwards() {
    let mut watch = Watch::default();
    watch.on(PAST, 10 * MINUTE);
    watch.on(PAST, 11 * MINUTE);
    let events = watch.on(UP, 5 * MINUTE);
    assert_eq!(events, [back(10 * MINUTE, 11 * MINUTE, causes(0, 0, 2, 0))]);
}

#[test]
fn verdicts_say_all_mostly_partly_or_mixed() {
    assert_eq!(Causes::default().verdict(), None);
    assert_eq!(causes(0, 0, 0, 4).verdict(), None);
    assert_eq!(
        causes(0, 2, 0, 0).verdict(),
        Some(Verdict::All(Cause::Router))
    );
    assert_eq!(
        causes(0, 2, 5, 0).verdict(),
        Some(Verdict::Mostly {
            cause: Cause::PastRouter,
            count: 5,
            total: 7
        })
    );
    assert_eq!(
        causes(0, 0, 3, 4).verdict(),
        Some(Verdict::Partly {
            cause: Cause::PastRouter,
            count: 3,
            total: 7
        })
    );
    assert_eq!(causes(0, 3, 3, 1).verdict(), Some(Verdict::Mixed));
    assert_eq!(causes(0, 1, 1, 0).verdict(), Some(Verdict::Mixed));
    assert_eq!(causes(1, 0, 1, 0).verdict(), Some(Verdict::Mixed));
}

#[test]
fn counts_saturate() {
    let mut full = causes(0, u32::MAX, u32::MAX, 0);
    full.add(Cause::Router);
    assert_eq!(full.router, u32::MAX);
    assert_eq!(full.total(), u32::MAX);
}

#[test]
fn causes_read_missing_and_unknown_keys() {
    let read: Causes =
        serde_json::from_str(r#"{"router":2,"pastRouter":5,"somethingNew":1}"#).unwrap();
    assert_eq!(read, causes(0, 2, 5, 0));
    assert_eq!(
        serde_json::to_value(read).unwrap(),
        serde_json::json!({"noNetwork": 0, "router": 2, "pastRouter": 5, "unknown": 0})
    );
}

/// The copy for every shape of outage. `tests/unit/outage.test.ts` checks the
/// frontend's sentence against the same file.
#[test]
fn copy_fixture_matches_the_wording() {
    let cases = [
        ("no checks", causes(0, 0, 0, 0)),
        ("only unclear", causes(0, 0, 0, 3)),
        ("all at the router", causes(0, 2, 0, 0)),
        ("all past the router", causes(0, 0, 7, 0)),
        ("all no network", causes(1, 0, 0, 0)),
        ("mostly at the router", causes(0, 5, 1, 1)),
        ("mostly past the router", causes(0, 2, 5, 0)),
        ("mostly no network", causes(3, 0, 1, 0)),
        ("partly past the router", causes(0, 0, 3, 4)),
        ("partly at the router", causes(0, 3, 0, 4)),
        ("partly no network", causes(1, 0, 0, 1)),
        ("mixed with unclear", causes(0, 3, 3, 1)),
        ("mixed without unclear", causes(2, 0, 2, 0)),
        ("two checks that disagree", causes(0, 1, 1, 0)),
        ("two checks past the router", causes(0, 0, 2, 0)),
    ];
    let values: Vec<_> = cases
        .iter()
        .map(|(name, causes)| {
            serde_json::json!({
                "name": name,
                "causes": causes,
                "sentence": causes.sentence(),
                "notice": causes.notice(),
                "place": causes.place(),
                "tally": causes.tally(),
                "key": causes.key(),
            })
        })
        .collect();
    let text = serde_json::to_string_pretty(&values).unwrap() + "\n";
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/outage-copy.json");
    if std::env::var_os("WEIR_UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    let saved = std::fs::read_to_string(&path)
        .unwrap()
        .replace("\r\n", "\n");
    assert!(
        saved == text,
        "fixtures/outage-copy.json is out of date: run `WEIR_UPDATE_FIXTURES=1 cargo test copy_fixture` and `pnpm test`"
    );
}

#[test]
fn copy_reads_plainly() {
    let mixed = causes(0, 3, 3, 1);
    assert_eq!(
        mixed.sentence().as_deref(),
        Some("Checks disagreed: 3 of 7 at your router, 3 past it, 1 unclear.")
    );
    assert_eq!(mixed.notice(), None);
    assert_eq!(mixed.place().as_deref(), Some("checks disagreed"));
    assert_eq!(
        mixed.tally().as_deref(),
        Some("checks: 3 at your router, 3 past your router, 1 unclear")
    );
    assert_eq!(mixed.key(), Some("mixed"));
    let mostly = causes(0, 2, 5, 0);
    assert_eq!(
        mostly.sentence().as_deref(),
        Some("Mostly past your router: 5 of 7 checks.")
    );
    assert_eq!(mostly.notice().as_deref(), Some("Mostly past your router."));
    assert_eq!(mostly.place().as_deref(), Some("mostly past your router"));
    assert_eq!(mostly.key(), Some("past_router"));
    let partly = causes(0, 0, 3, 4);
    assert_eq!(
        partly.sentence().as_deref(),
        Some("Past your router in 3 of 7 checks. The rest were unclear.")
    );
    assert_eq!(partly.notice(), None);
    assert_eq!(partly.key(), Some("past_router"));
    assert_eq!(
        causes(0, 0, 2, 0).sentence().as_deref(),
        Some("Your router was reachable throughout. The break was past it.")
    );
    assert_eq!(
        causes(2, 0, 0, 0).place().as_deref(),
        Some("no network on this computer")
    );
    assert_eq!(
        causes(0, 0, 0, 3).tally().as_deref(),
        Some("checks: 3 unclear")
    );
    for none in [Causes::default(), causes(0, 0, 0, 3)] {
        assert_eq!(none.sentence(), None);
        assert_eq!(none.notice(), None);
        assert_eq!(none.place(), None);
        assert_eq!(none.key(), None);
    }
    assert_eq!(Causes::default().tally(), None);
}
