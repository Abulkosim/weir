use super::*;

const MINUTE: u64 = 60_000;

fn down(cause: Cause) -> Check {
    Check::Down(cause)
}

const UP: Check = Check::Up { router_now: None };
const FAIL: Check = Check::Down(Cause::Unknown);
const TICK_OFFLINE: Check = Check::Offline { from_path: false };
const PATH_OFFLINE: Check = Check::Offline { from_path: true };

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
fn signals_classify_where_it_broke() {
    let cases = [
        (None, None, Cause::Unknown),
        (None, Some(true), Cause::Unknown),
        (Some(true), None, Cause::PastRouter),
        (Some(true), Some(false), Cause::PastRouter),
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
    assert!(watch.on(down(Cause::PastRouter), MINUTE).is_empty());
    assert_eq!(watch.causes(), causes(0, 0, 1, 0));
    assert!(watch.on(UP, 2 * MINUTE).is_empty());
    assert_eq!(watch.outage_since_ms(), None);
    assert!(watch.causes().is_empty(), "a lone failure leaves no tally");
}

#[test]
fn two_failures_start_an_outage_dated_from_the_first_and_both_count() {
    let mut watch = Watch::default();
    watch.on(UP, 0);
    assert!(watch.on(down(Cause::PastRouter), MINUTE).is_empty());
    assert_eq!(
        watch.on(down(Cause::PastRouter), 2 * MINUTE),
        [Event::Lost { since_ms: MINUTE }]
    );
    assert!(watch.on(down(Cause::Unknown), 3 * MINUTE).is_empty());
    assert_eq!(
        watch.on(UP, 5 * MINUTE),
        [back(MINUTE, 5 * MINUTE, causes(0, 0, 2, 1))]
    );
    assert_eq!(watch.outage_since_ms(), None);
    assert!(watch.causes().is_empty());
}

#[test]
fn losing_every_path_is_an_outage_at_once() {
    let mut watch = Watch::default();
    watch.on(UP, 0);
    assert_eq!(
        watch.on(PATH_OFFLINE, 30_000),
        [Event::Lost { since_ms: 30_000 }]
    );
    assert!(watch.on(PATH_OFFLINE, 40_000).is_empty());
    assert!(watch.on(FAIL, MINUTE).is_empty());
    assert_eq!(
        watch.on(UP, 2 * MINUTE),
        [back(30_000, 2 * MINUTE, causes(1, 0, 0, 1))]
    );
}

#[test]
fn a_burst_of_path_updates_counts_once_but_minute_ticks_each_count() {
    let mut watch = Watch::default();
    for second in 0..5 {
        watch.on(PATH_OFFLINE, second * 1_000);
    }
    assert_eq!(watch.causes(), causes(1, 0, 0, 0));
    watch.on(TICK_OFFLINE, MINUTE);
    watch.on(TICK_OFFLINE, 2 * MINUTE);
    watch.on(PATH_OFFLINE, 2 * MINUTE + 1);
    assert_eq!(watch.causes(), causes(3, 0, 0, 0));
    let mut after_failure = Watch::default();
    after_failure.on(down(Cause::PastRouter), 0);
    after_failure.on(PATH_OFFLINE, 1_000);
    after_failure.on(PATH_OFFLINE, 2_000);
    assert_eq!(
        after_failure.causes(),
        causes(1, 0, 1, 0),
        "losing the network after a failed check still counts, once"
    );
    assert_eq!(after_failure.outage_since_ms(), Some(0));
}

#[test]
fn a_short_wifi_drop_after_one_failed_check_is_not_blamed_past_the_router() {
    let mut watch = Watch::default();
    watch.on(UP, 0);
    watch.on(down(Cause::PastRouter), MINUTE);
    assert_eq!(
        watch.on(PATH_OFFLINE, MINUTE + 20_000),
        [Event::Lost { since_ms: MINUTE }]
    );
    let events = watch.on(UP, MINUTE + 50_000);
    assert_eq!(events, [back(MINUTE, MINUTE + 50_000, causes(1, 0, 1, 0))]);
    let Event::Back { causes, .. } = events[0] else {
        unreachable!()
    };
    assert_eq!(causes.verdict(), Some(Verdict::Mixed));
}

#[test]
fn a_new_spell_without_network_counts_again() {
    let mut watch = Watch::default();
    watch.on(PATH_OFFLINE, 0);
    watch.on(PATH_OFFLINE, 1_000);
    watch.on(down(Cause::Router), MINUTE);
    watch.on(PATH_OFFLINE, MINUTE + 1_000);
    watch.on(PATH_OFFLINE, MINUTE + 2_000);
    assert_eq!(watch.causes(), causes(2, 1, 0, 0));
}

#[test]
fn a_check_that_straddles_a_network_change_is_unclear() {
    let silent = Signals {
        router: Some(false),
        dns: None,
    };
    assert_eq!(classify_on(silent, true), Cause::Router);
    assert_eq!(classify_on(silent, false), Cause::Unknown);
    let answered = Signals {
        router: Some(true),
        dns: None,
    };
    assert_eq!(classify_on(answered, false), Cause::Unknown);
}

#[test]
fn a_sleep_gap_closes_an_outage_where_it_was_last_seen() {
    let mut watch = Watch::default();
    watch.on(down(Cause::Router), 0);
    watch.on(down(Cause::PastRouter), MINUTE);
    let after_sleep = 3 * 60 * MINUTE;
    assert_eq!(
        watch.on(
            Check::Up {
                router_now: Some(true)
            },
            after_sleep
        ),
        [back(0, MINUTE, causes(0, 0, 1, 1))],
        "router blame can't be re-checked across a sleep"
    );
    let mut quiet = Watch::default();
    quiet.on(FAIL, 0);
    assert!(
        quiet.on(down(Cause::Router), after_sleep).is_empty(),
        "a failure before sleep and one after are not two in a row"
    );
    assert_eq!(quiet.causes(), causes(0, 1, 0, 0), "the tally starts fresh");
}

#[test]
fn router_blame_stands_only_when_the_router_answers_after() {
    let run = |router_now| {
        let mut watch = Watch::default();
        watch.on(down(Cause::Router), 0);
        watch.on(down(Cause::Router), MINUTE);
        watch.on(down(Cause::PastRouter), 2 * MINUTE);
        assert!(watch.router_recheck_due());
        watch.on(Check::Up { router_now }, 3 * MINUTE)
    };
    assert_eq!(run(Some(true)), [back(0, 3 * MINUTE, causes(0, 2, 1, 0))]);
    assert_eq!(
        run(Some(false)),
        [back(0, 3 * MINUTE, causes(0, 0, 1, 2))],
        "a router silent with the internet back proves nothing by its silence"
    );
    assert_eq!(run(None), [back(0, 3 * MINUTE, causes(0, 0, 1, 2))]);
}

#[test]
fn a_network_change_makes_router_blame_unclear() {
    let mut watch = Watch::default();
    watch.on(down(Cause::Router), 0);
    watch.on(down(Cause::Router), MINUTE);
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
    calm.on(down(Cause::Router), 3 * MINUTE);
    calm.on(down(Cause::Router), 4 * MINUTE);
    assert!(
        calm.router_recheck_due(),
        "a change before any failure doesn't count"
    );
}

#[test]
fn no_recheck_is_due_without_router_blame() {
    let mut watch = Watch::default();
    assert!(!watch.router_recheck_due());
    watch.on(down(Cause::Router), 0);
    assert!(!watch.router_recheck_due(), "no outage yet");
    let mut past = Watch::default();
    past.on(down(Cause::PastRouter), 0);
    past.on(down(Cause::PastRouter), MINUTE);
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
        let check = if *ok { UP } else { down(Cause::PastRouter) };
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
fn an_answer_from_before_the_network_went_away_does_not_end_the_outage() {
    let mut watch = Watch::default();
    watch.on(down(Cause::Router), 0);
    watch.on(down(Cause::Router), MINUTE);
    let started = 2 * MINUTE;
    assert!(watch.on(PATH_OFFLINE, started + 2_000).is_empty());
    let late_up = Check::Up {
        router_now: Some(true),
    };
    assert!(
        watch.on_from(late_up, started, started + 6_000).is_empty(),
        "the router re-check ran past the Wi-Fi drop"
    );
    assert_eq!(watch.outage_since_ms(), Some(0));
    assert_eq!(
        watch.on_from(UP, 3 * MINUTE, 3 * MINUTE),
        [back(0, 3 * MINUTE, causes(1, 0, 0, 2))]
    );
}

#[test]
fn a_down_from_before_the_network_went_away_still_counts() {
    let mut watch = Watch::default();
    watch.on(PATH_OFFLINE, 1_000);
    watch.on(UP, 2_000);
    assert_eq!(
        watch.on_from(FAIL, 0, 3_000),
        [],
        "one failure isn't an outage"
    );
    assert_eq!(watch.causes(), causes(0, 0, 0, 1));
}

#[test]
fn reset_forgets_an_open_outage_and_its_tally() {
    let mut watch = Watch::default();
    watch.on(TICK_OFFLINE, 0);
    watch.on(down(Cause::Router), 1);
    watch.reset();
    assert_eq!(watch.outage_since_ms(), None);
    assert!(watch.causes().is_empty());
    assert!(watch.on(UP, MINUTE).is_empty());
}

#[test]
fn checks_out_of_order_never_end_before_they_start() {
    let mut watch = Watch::default();
    watch.on(FAIL, 10 * MINUTE);
    watch.on(FAIL, 11 * MINUTE);
    let events = watch.on(UP, 9 * MINUTE);
    assert_eq!(events, [back(10 * MINUTE, 11 * MINUTE, causes(0, 0, 0, 2))]);
}

#[test]
fn verdicts_name_the_dominant_place_and_admit_a_mix() {
    assert_eq!(Causes::default().verdict(), None);
    assert_eq!(causes(0, 0, 0, 4).verdict(), None);
    assert_eq!(
        causes(0, 3, 0, 0).verdict(),
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
        causes(0, 3, 0, 4).verdict(),
        Some(Verdict::Partly {
            cause: Cause::Router,
            count: 3,
            total: 7
        })
    );
    assert_eq!(causes(0, 3, 3, 1).verdict(), Some(Verdict::Mixed));
    assert_eq!(causes(0, 1, 1, 0).verdict(), Some(Verdict::Mixed));
    let mut full = causes(u32::MAX, 0, 0, 0);
    full.add(Cause::NoNetwork);
    assert_eq!(full.total(), u32::MAX, "counts saturate");
}

#[test]
fn copy_says_where_it_broke() {
    let all_past = causes(0, 0, 7, 0);
    assert_eq!(
        all_past.sentence().as_deref(),
        Some("Your router was reachable throughout. The break was past it.")
    );
    assert_eq!(
        all_past.notice().as_deref(),
        Some("The break was past your router.")
    );
    assert_eq!(all_past.place().as_deref(), Some("past your router"));
    assert_eq!(all_past.key(), Some("past_router"));
    let mixed = causes(0, 3, 3, 1);
    assert_eq!(
        mixed.sentence().as_deref(),
        Some("Checks disagreed: 3 of 7 at your router, 3 past it, 1 unclear.")
    );
    assert_eq!(mixed.notice(), None);
    assert_eq!(
        mixed.tally().as_deref(),
        Some("checks: 3 at your router, 3 past your router, 1 unclear")
    );
    assert_eq!(Causes::default().tally(), None);
    assert_eq!(causes(0, 0, 0, 2).sentence(), None);
    assert_eq!(causes(0, 0, 0, 2).key(), None);
    assert_eq!(causes(0, 2, 5, 0).key(), Some("past_router"));
    assert_eq!(
        causes(0, 3, 0, 4).key(),
        None,
        "router in 3 of 7 is no verdict for scripts"
    );
    assert_eq!(causes(0, 3, 3, 1).key(), Some("mixed"));
}

/// Every verdict's copy, read by `tests/unit/outage.test.ts` so History says the same.
#[test]
fn copy_fixture_matches_what_the_app_says() {
    let cases = [
        ("empty", Causes::default()),
        ("unclear only", causes(0, 0, 0, 3)),
        ("all router", causes(0, 4, 0, 0)),
        ("all past router", causes(0, 0, 7, 0)),
        ("all no network", causes(3, 0, 0, 0)),
        ("mostly router", causes(0, 5, 1, 1)),
        ("mostly past router", causes(0, 2, 5, 0)),
        ("mostly no network", causes(3, 0, 1, 0)),
        ("partly router", causes(0, 3, 0, 4)),
        ("partly past router, 2 checks", causes(0, 0, 1, 1)),
        ("mixed with unclear", causes(0, 3, 3, 1)),
        ("mixed, 2 checks", causes(0, 1, 1, 0)),
        ("mixed past router and no network", causes(2, 0, 2, 1)),
    ];
    let rows: Vec<serde_json::Value> = cases
        .iter()
        .map(|(name, causes)| {
            serde_json::json!({
                "name": name,
                "causes": causes,
                "sentence": causes.sentence(),
                "notice": causes.notice(),
                "place": causes.place(),
                "key": causes.key(),
                "tally": causes.tally(),
            })
        })
        .collect();
    let text = serde_json::to_string_pretty(&rows).unwrap() + "\n";
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/outage-copy.json");
    if std::env::var_os("WEIR_UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    let saved = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        saved == text,
        "fixtures/outage-copy.json is out of date: run `WEIR_UPDATE_FIXTURES=1 cargo test copy_fixture` and `pnpm test`"
    );
}
