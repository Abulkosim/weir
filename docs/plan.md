# Weir — plan

Written 2026-10-23 from [research.md](research.md), [method.md](method.md) and the owner's answers. This file is the work queue. One task per iteration, smallest shippable slice. Tick when the **Verify** line passes; write findings under the task. Owner-only items are marked 👤: prepared, left unchecked, never done on the owner's behalf. Release boundaries are marked ⛳.

## Owner decisions (2026-10-23)

- Name **Weir**, repo `Abulkosim/weir`, MIT.
- Backend: Cloudflare only in v0.1. fast.com and Ookla: no. M-Lab and macOS `networkQuality`: not planned (owner didn't pick them).
- Background monitoring: **off** in v0.1. Full tests are manual. The OS network-path monitor (no traffic) is still used for offline and metered state.
- Tray: **health ring**. Fills while testing; at rest shows the last result's health, dims when stale, breaks when offline. Speed text beside it on mac, off by default.
- Look: Margin's structure and quality bar, new palette: **Tide** (cool slate neutrals, teal accent, Geist).
- Repo public from day one.
- History: every test, kept 1 year, CSV export in v0.1. PDF provider report later.
- Commits: Conventional Commits with a body. No trailers, no AI attribution. Author `abulkosim <abulkasimkattaev@gmail.com>`.

## Rules for the loop

- Before every commit, all green: `pnpm typecheck && pnpm test && pnpm test:e2e`, then in `src-tauri`: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`.
- UI change → Playwright screenshots light + dark to the test output path; look at them; refresh `docs/images` when visible copy changes.
- No real network in tests. Measurement math is pure and tested on recorded fixtures; backends are tested against a local mock server.
- Real-network checks only on the owner's Mac, sparingly, never on a metered link. Results in `docs/verification.md` without IPs, cities or SSIDs.
- Never store or log `cf-meta-ip`, city, ASN, coordinates, SSID or BSSID.
- Copy: short, plain, units always. No exclamation marks, no "boost".

## Phase 0 — Direction and skeleton

- [x] **0.1 Three visual directions.** Static HTML prototypes of the popover (result state, testing state) in light and dark: new palette, accent, display type. Screenshot via Playwright into `design/directions/`.
  Verify: 3 × 2 × 2 screenshots exist and are shown to the owner 👤 who picks one.
  Done 2026-10-23: owner picked **Tide** (cool slate, teal accent, Geist). `design/directions/index.html` is the reference.
- [x] **0.2 Scaffold.** Tauri 2 + Vue 3 + TS + Vite + pnpm, ported from Margin: `package.json`, `tsconfig`, `vite.config`, Playwright config, `src-tauri` with tray, single-instance, positioner, autostart, updater (inert until keys exist). App id `com.abulkosim.weir`. LICENSE, `.gitignore`, `deny.toml`.
  Verify: `pnpm tauri dev` shows a tray icon and an empty popover on mac; all checks green.
  Done 2026-10-23: app id `com.abulkosim.weir`, dev port 1430 (1420 is taken by another local project). Popover geometry split into `core/popover.rs` with Margin's placement tests re-derived for 380×600. Dev run: popover window 380 wide placed under the tray, hides on blur. Updater plugin left out until the key exists (3.3); tray ring is Margin's renderer until 1.5.
- [x] **0.3 Design tokens.** Chosen direction as `src/styles.css` tokens (light + dark), `src/design/controls.css`, `components/ui/*` ported from Margin and restyled. Inter + chosen display face, tabular numbers.
  Verify: a demo page renders every control in both themes; screenshots looked at.
  Done 2026-10-23: Tide tokens in `src/styles.css` (light + dark), Geist Variable bundled via `@fontsource-variable/geist` (no runtime font fetch), Margin's switch/select/tooltip ported, Weir icon set. `?demo=1&state=design` gallery; `tests/unit/contrast.test.ts` asserts AA for every text token on surface and card in both themes.
- [x] **0.4 Repo on GitHub.** Create `Abulkosim/weir`, push `main`, CI skeleton (frontend job only).
  Verify: CI green on `main`.
  Done 2026-10-23: public repo, `ci.yml` runs frontend + Playwright on Ubuntu and Rust fmt/clippy/test on macOS and Windows; Dependabot on.

## Phase 1 — M1 Alive: a real test from the tray

- [x] **1.1 Measurement math.** `core/stats.rs`: median, percentiles, mean |Δ| jitter, warm-up detection, stop rule, steady-window mean, upload per-request rate, validity rules, data estimate. Pure functions. (Plain-meaning rules are 2.3.)
  Verify: unit tests on recorded sample fixtures (steady, slow ramp, stall, cap hit, spiky, too short) — each gives the expected number or the expected failure.
  Done 2026-10-23: 18 tests. Fixtures are synthetic shapes (`scripts/make-throughput-fixtures.py`) until real recordings exist (1.6). Steady 300 Mbps converges at ~5.3 s within 1 %; spiky link reports its mean, not the peak, and is marked varied; stall fails at ~5.5 s; gigabit hits the 380 MB cap at ~3.6 s. Caps became 380/190 MB so the shown ceiling, with header overhead, is honestly 600 MB.
- [x] **1.2 Backend trait + Cloudflare client.** `core/backends/{mod,cloudflare}.rs`: trace/colo, latency probe with `Server-Timing` subtraction, download and upload streams, HTTP/1.1, caps, cancel. Base URL injectable.
  Verify: wiremock tests for success, slow, stalled, cut-off mid-body, 403, 429 with `Retry-After`, 5xx; no response header besides `colo` reaches the result struct.
  Done 2026-10-23: `Backend`/`Connection` traits (one HTTP/1.1 client per stream), `Cloudflare` with injectable base. 9 tests incl. a raw TCP server that cuts the body at 40 of 100 kB. Upload sends a fixed Content-Length (no chunked encoding) and counts chunks as hyper pulls them.
- [x] **1.3 Test runner.** `core/speedtest.rs`: sequence pre-flight → idle → download (+ loaded probes) → upload, progress events at 10 Hz, cancel, network-change abort, result or typed failure.
  Verify: runner test against the mock gives a full result; a mock stall gives "Connection stalled" and no numbers; cancel leaves no result.
  Done 2026-10-23: `core/speedtest.rs`, tested with a fake backend on tokio's paused clock (a full 15 s test runs in ~40 ms). 7 tests: full result (300/40 Mbps, 15 ms idle, 60 ms loaded), stall, cancel, network change, offline, refused upload keeps the download, distant server gets 8+6 streams. Upload requests target 0.5 s (download 1 s) so each stream finishes ≥ 3 requests inside the steady window.
- [ ] **1.4 IPC + popover.** Commands `start_test`, `cancel_test`, `last_result`; progress events; Zod schemas for every payload. Popover: result card (download, upload, latency · under load, jitter, server, time), Test button, live gauge while running, numbers counting up. `fit_popover` + ResizeObserver from Margin.
  Verify: e2e in demo bridge for never-tested, testing, result; light + dark screenshots looked at.
- [ ] **1.5 Tray ring.** Template ring on mac (alpha only), themed on Windows by taskbar setting; fills with progress while testing; health at rest. Rendered in Rust like Margin's `ring.rs`.
  Verify: contact sheet at 16/22/32 px light + dark in `docs/images/tray/`; contrast ≥ 3:1 asserted in a test.
- [ ] **1.6 Real check 👤.** One test on the owner's Mac on an unmetered link, compared with speed.cloudflare.com back to back, 3 runs each.
  Verify: results in `docs/verification.md` (numbers and colo only); within ~10 % at ≤ 300 Mbps, or the gap explained.

## Phase 2 — M2 Real: everything v0.1 needs

- [ ] **2.1 Metered guard.** `core/metered.rs`: macOS `nw_path_monitor` (expensive, constrained, satisfied) via a small `extern "C"` binding; Windows `GetInternetConnectionProfile().GetConnectionCost()` (Unknown = metered). Test button asks first with the data estimate.
  Verify: unit tests on the decision table; e2e for the metered confirm state; manual check on this Mac's hotspot (no test run) 👤-free.
- [ ] **2.2 Offline state.** Path unsatisfied → "No internet since 14:02", Test disabled, tray ring broken. Network change during a test aborts it.
  Verify: unit test on transitions; e2e for offline.
- [ ] **2.3 Plain meaning.** Headline sentence and per-topic lines from method.md rules under the result.
  Verify: unit tests per threshold boundary; e2e for good, calls-stutter and slow results.
- [ ] **2.4 Every failure state designed.** Failed (each validity message), cancelled, stale result (> 24 h: "Tested 2 days ago"), cap hit, "varied" range, latency not measured.
  Verify: one demo state each; e2e + screenshots light + dark.
- [ ] **2.5 History.** `core/history.rs`: own bounded JSON file, 1 year retention, atomic writes, tolerant load. Popover list (date, down, up, latency, server), small download sparkline.
  Verify: unit tests for pruning, corrupt-file recovery, size bound; e2e history state.
- [ ] **2.6 CSV export.** Save dialog, columns documented in README; no IP, city or SSID.
  Verify: unit test on CSV output for a fixture history; manual save once.
- [ ] **2.7 Settings.** Versioned JSON, tolerant per-field loading, recovery note, back up before reset. Sections: Appearance (System/Light/Dark), Tray (speed text on mac), Data (show estimate before each test), Launch at login, History (export, clear), Updates, About.
  Verify: unit tests for migration and one bad field; e2e for settings in both themes.
- [ ] **2.8 Windows popover.** Mica on Win11 else solid, 8 px radius, tray anchoring, DPI 125/150/200 %.
  Verify: CI Windows build green; manual VM check 👤.
- [ ] **2.9 Accessibility pass.** Full keyboard path, focus ring, screen-reader labels on numbers ("Download 312 megabits per second"), `prefers-reduced-motion`.
  Verify: e2e keyboard walk; axe check has no serious issues.

## Phase 3 — M3 Ship v0.1.0 ⛳

- [ ] **3.1 CI full.** Port Margin's `ci.yml` (Ubuntu frontend + Playwright 45 min, macOS universal DMG + `.app.tar.gz`, Windows NSIS + MSI), `security.yml`, `scorecard.yml`, Dependabot. Actions pinned by SHA, least privilege, concurrency.
  Verify: all jobs green on `main`.
- [ ] **3.2 Release tooling.** Port `scripts/` (check-release, checksums, collect-artifacts, updater-manifest, updater-signature) and `docs/releasing.md`; tag job: sign updater artifacts, `latest.json`, `SHA256SUMS.txt`, SBOM, provenance, draft release.
  Verify: script tests pass; dry-run on a test tag in a fork-free way (workflow_dispatch) produces a draft.
- [ ] **3.3 Updater key 👤.** `pnpm tauri signer generate`, stored outside the repo with a backup, `TAURI_SIGNING_PRIVATE_KEY` secret. Prepared command and steps in `docs/releasing.md`.
- [ ] **3.4 README + docs.** Hero screenshots, 3-bullet value, Install in the first screen, comparison table vs fast.com, Ookla, speed.cloudflare.com, `networkQuality`. `docs/install.md` (Open Anyway, SmartScreen), PRIVACY.md (hosts contacted, what is stored), SECURITY.md, CONTRIBUTING.md.
  Verify: links checked; screenshots current.
- [ ] **3.5 Pages site.** `site/` from Margin's template, new palette.
  Verify: `pages.yml` deploys; 👤 enable Pages source "GitHub Actions".
- [ ] **3.6 Release v0.1.0.** Changelog, tag, draft, checksums, updater install test (tampered bytes and wrong version rejected), `docs/verification.md`, report; 👤 owner says "publish".

## Phase 4 — Is it me or the internet? (v0.2) ⛳

- [ ] **4.1 Diagnose.** `core/diagnose.rs`: Wi-Fi (RSSI, noise, rate; never SSID) → router (gateway ping: ICMP DGRAM on mac, `IcmpSendEcho2` on Windows, TCP connect fallback) → DNS (configured server query + system lookup) → internet (`cp.cloudflare.com/generate_204`). Each step one plain sentence: "Your Wi-Fi is fine. The problem is past your router."
- [ ] **4.2 Designed states:** Wi-Fi weak, router unreachable, DNS slow, captive portal.
- [ ] **4.3 Light background checks (opt-in).** Every 60 s ±10 % on power, 2–5 min on battery, < 1 KB/min, event-driven on path change. Outage timeline "No internet 14:02–14:09". Notification "Connection lost / back", off by default.
- [ ] **4.4 Slow-hours chart** from history (by hour of day).

## Phase 5 — Later

- [ ] PDF provider report (tests, outages, against the speed you pay for).
- [ ] "Speed you pay for" setting and comparison.
- [ ] Per-network history keyed by a salted hash of the gateway MAC, user-named.
- [ ] Scheduled full tests (opt-in, never metered).
- [ ] Global shortcut; `weir --json`.
- [ ] Homebrew tap and winget jobs, gated off (Margin's `packaging/`).

## Not doing

- fast.com (unofficial API, Netflix ToS), Ookla CLI (EULA), packet loss via TURN (needs a paid server), reading SSID (Location prompt), any telemetry or analytics, paid signing.
