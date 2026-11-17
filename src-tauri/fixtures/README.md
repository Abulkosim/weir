# Fixtures

`throughput/` holds byte-counter recordings for `core/stats.rs` tests: `[elapsed ms, cumulative bytes]` pairs. Current files are synthetic shapes from `scripts/make-throughput-fixtures.py`; recorded runs from real tests can be added next to them. Never include IPs, cities, ASNs or SSIDs.

`ipc.json` is every field the popover receives from Rust, with each optional part filled in. The Rust test `ipc_fixture_matches_what_the_app_sends` writes it (`WEIR_UPDATE_FIXTURES=1 cargo test ipc_fixture`) and fails when it is stale; `tests/unit/ipc.test.ts` checks the frontend's Zod schemas read all of it and drop nothing.

`outage-copy.json` is what Weir says about where an outage broke, for each kind of mix of checks. The Rust test `copy_fixture_matches_what_the_app_says` writes it (`WEIR_UPDATE_FIXTURES=1 cargo test copy_fixture`) and fails when it is stale; `tests/unit/outage.test.ts` checks History says the same. Counts only, never an address.
