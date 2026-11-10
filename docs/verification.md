# Verification

What was actually checked, newest release on top. Date, command, platform and outcome. No IP addresses, cities or Wi-Fi names.

## v0.7.3

**[v0.7.3 is published](https://github.com/Abulkosim/weir/releases/tag/v0.7.3)** at 2026-11-10 20:34 UTC and marked latest; `latest.json` serves 0.7.3. Published on standing approval. Tag `v0.7.3` builds `acfcee2`. Sweep patch.

- A read-only audit of the Rust core and the frontend, then one pull request per finding, each green before merging: #34–#49, and #50 (release prep). What each fixed is in [plan.md](plan.md) under S.1 and S.2.
- Locally on Linux, on main with every change combined: types, 46 unit tests, 14 release script tests, fmt, strict Clippy, 131 Rust tests, 202 browser cases (Chromium + WebKit), `check-release.mjs`. Two WebKit dark accessibility cases failed once under load on the #44 branch and passed 20/20 on repeat; they passed in the combined run. Windows and macOS code built and tested in CI only.
- New checks: the Rust IPC fixture and `tests/unit/ipc.test.ts` keep the Zod schemas in step with what Rust sends; format, live-chart, History-key and median unit tests; store tests for files that can't be opened or come from a newer Weir.
- [Main CI](https://github.com/Abulkosim/weir/actions/runs/42892985547) and the [tag build](https://github.com/Abulkosim/weir/actions/runs/42894329987) green on every job.
- The [updater test in the tag build](https://github.com/Abulkosim/weir/actions/runs/42894329987/job/130107636554) rejected altered bytes and a wrong signed version and installed 0.7.3 for x86_64 and arm64. Its archive checksum `154311b551cb8601dc3593033542283d493e775b2bdf1394196025f787821412` matches `SHA256SUMS.txt`.
- All eight checksummed files matched; `gh attestation verify` passed for the DMG, Mac archive, EXE, MSI, `latest.json` and `SHA256SUMS.txt`.
- DMG SHA-256 `056250c045a8a534d77fef20cf76b7828d071b973971014141a011efcf50ae90`.
- Homebrew tap bumped by hand (`weir 0.7.3`, `94b8720`); release DMG URL answers 200. `brew style` not run (no Homebrew on this machine).
- Pending: 👤 update the installed app in-app (or `brew upgrade --cask --greedy`); 👤 Windows VM check; 👤 real-network check (1.6); 👤 tap deploy key; 👤 branch protection.

## v0.7.2

**[v0.7.2 is published](https://github.com/Abulkosim/weir/releases/tag/v0.7.2)** at 2026-11-10 16:28 UTC and marked latest; `latest.json` serves 0.7.2. Published by the owner's go-ahead in chat. Tag `v0.7.2` builds `188b559`. Patch for the two fuzzing findings.

- Pull requests, each green before merging: #27 (updater test in tag builds), #28 (CodeQL), #29 (fuzzing, History float round trip, `Server-Timing` overflow), #30 (frontend), #31 (actions), #32 (release prep).
- Locally on Linux: types, 37 unit tests, 14 release script tests, `check-release.mjs`. Rust fmt, Clippy and tests were stopped (the machine ran out of memory) and WebKit cases could not run (the new Playwright's WebKit did not download); both ran green in CI on #32, main and the tag.
- [Main CI](https://github.com/Abulkosim/weir/actions/runs/42842967246) green after rerunning the macOS job, which GitHub cancelled for lack of a macOS runner. [Tag build](https://github.com/Abulkosim/weir/actions/runs/42851160362) green on every job.
- First tag build with the [updater test built in](https://github.com/Abulkosim/weir/actions/runs/42851160362/job/129956945441): rejected altered bytes and a wrong signed version and installed 0.7.2 for x86_64 and arm64. Its archive checksum `2981ffafeddcf0885c50ab9a422386072fb59ae845328079d2f0d62bb4cc5ff7` matches `SHA256SUMS.txt`.
- All eight checksummed files matched; `gh attestation verify` passed for the DMG, Mac archive, EXE, MSI, `latest.json` and `SHA256SUMS.txt`.
- DMG SHA-256 `b432e698ef4df100f697b80670bbe03961389cf241c13b9f27d99ab2c3aa6c8a`.
- Homebrew tap bumped by hand (`weir 0.7.2`, `38dad7c`); release DMG URL answers 200. `brew style` not run (no Homebrew on this machine).
- Pending: 👤 update the installed app in-app (or `brew upgrade --cask --greedy`); 👤 Windows VM check; 👤 tap deploy key.

## v0.7.1

**[v0.7.1 is published](https://github.com/Abulkosim/weir/releases/tag/v0.7.1)** at 2026-11-07 15:00 UTC and marked latest; `latest.json` serves 0.7.1. Published on standing approval. Tag `v0.7.1` builds `c060972`. Dependency patch.

- Pull requests, each green before merging: #24 (yanked `async-executor` 1.14.0 → 1.14.1; it failed `cargo deny` on every Dependabot pull request), #21 (actions), #22 (frontend), #23 (Tauri plugins), #18 (`netdev` 0.47), #25 (release prep and `updater-test.yml`).
- Locally on Linux, on main with every update combined: 200 browser cases (Chromium + WebKit), 37 unit tests, 18 release and cask script tests, 118 Rust tests, fmt, strict Clippy, `check-release.mjs`. Windows and macOS code built and tested in CI only.
- [Main CI](https://github.com/Abulkosim/weir/actions/runs/42382750333) and the [tag build](https://github.com/Abulkosim/weir/actions/runs/42383350847) green on every job. Scorecard failed on main while Sigstore's transparency log returned 500 (Sigstore incident, database upgrade); not caused by these changes.
- All eight checksummed files matched; `gh attestation verify` passed for the DMG, Mac archive, EXE, MSI, `latest.json` and `SHA256SUMS.txt`.
- The [updater test on a macOS runner](https://github.com/Abulkosim/weir/actions/runs/42383962071) rejected altered bytes and a wrong signed version and installed 0.7.1 for x86_64 and arm64. Its archive checksum `6573865d205cdb4d86ff7ad00f8e4db54e3ef7d0bc2c73f0cabd12ce551b3cf3` matches `SHA256SUMS.txt`.
- DMG SHA-256 `82913d6e911f0dea45e8fa15d3071d8018f9b7160ccb3dcf23cbc91d7f7de5b7`.
- Homebrew tap bumped by hand (`weir 0.7.1`, `5dcfa2e`); release DMG URL answers 200. `brew style` not run (no Homebrew on this machine).
- Pending: 👤 update the installed app in-app (or `brew upgrade --cask --greedy`); 👤 Windows VM check; 👤 tap deploy key.

## v0.7.0

**[v0.7.0 is published](https://github.com/Abulkosim/weir/releases/tag/v0.7.0)** at 2026-11-01 10:21 UTC and marked latest; `latest.json` serves 0.7.0. Published on standing approval. Tag `v0.7.0` builds `889ed14`.

- One pull request, green before merging: #20 (This month card in History).
- Locally: 200 browser cases (Chromium + WebKit), 37 unit tests (5 new for the month), 18 release and cask script tests, 120 Rust tests, fmt, strict Clippy, `check-release.mjs`, `brew style`. One WebKit popover-fit case failed once under load and passed 40/40 on repeat (not History).
- History screenshots looked at in light and dark; README image refreshed with a fixed mid-month date.
- [Main CI](https://github.com/Abulkosim/weir/actions/runs/41295330605) and the [tag build](https://github.com/Abulkosim/weir/actions/runs/41295930596) green on every job.
- All eight checksummed files matched; `gh attestation verify` passed for the DMG, Mac archive, EXE, MSI, `latest.json` and `SHA256SUMS.txt`; the real updater test rejected altered bytes and a wrong signed version and installed 0.7.0 for x86_64 and arm64.
- DMG SHA-256 `a7511cdb66b1c49c215f4a785efe7215ac383699415c3765669ac29ce2c35491`.
- Homebrew tap bumped by hand (`weir 0.7.0`); `brew upgrade --cask --greedy` installed 0.7.0 on the owner's Mac. A Homebrew reinstall is quarantined again, so the first launch shows the Gatekeeper prompt; answering it with Move to Trash removed the app (system log: `moveItemToTrash`). Reinstalled; 👤 first launch needs Open Anyway.
- Pending: 👤 open 0.7.0 once through Open Anyway; 👤 Windows VM check; 👤 tap deploy key.

## v0.6.0

**[v0.6.0 is published](https://github.com/Abulkosim/weir/releases/tag/v0.6.0)** at 2026-10-29 12:29 UTC and marked latest; `latest.json` serves 0.6.0. Published on standing approval. Tag `v0.6.0` builds `8745825`. The last planned release.

- One pull request, green before merging: #16 (`weir --json`, `weir history`, `weir test`).
- Locally: 32 unit tests, 18 release and cask script tests, 120 Rust tests, fmt, strict Clippy, `check-release.mjs`, `brew style` on the cask. Browser cases ran in CI only (no UI change).
- [Main CI](https://github.com/Abulkosim/weir/actions/runs/40870199428) and the [tag build](https://github.com/Abulkosim/weir/actions/runs/40872062785) green on every job.
- All eight checksummed files matched; `gh attestation verify` passed for the DMG, Mac archive, EXE, MSI, `latest.json` and `SHA256SUMS.txt`; the real updater test rejected altered bytes and a wrong signed version and installed 0.6.0 for x86_64 and arm64.
- DMG SHA-256 `0bb722a4b90df395677401da4c8ec52692069c70472005e611feb96134e8a9ab`.
- Homebrew tap bumped by hand (`weir 0.6.0`, now with the `binary` stanza); `brew upgrade --cask --greedy` installed 0.6.0 on the owner's Mac, `weir` on `PATH`, `weir --json` read the real History.
- On the owner's Mac before the release: `weir test` refused the hotspot (exit 3). On another connection, after a burst of size probes, Cloudflare refused downloads of 15 MB and up (403); `weir test` reported `refused`. Not caused by this release.
- Pending: 👤 Windows VM check of `Weir.exe --json` in a terminal; 👤 tap deploy key (`docs/releasing.md`).

## v0.5.0

**[v0.5.0 is published](https://github.com/Abulkosim/weir/releases/tag/v0.5.0)** at 2026-10-28 18:21 UTC and marked latest; `latest.json` serves 0.5.0. Published on standing approval. Tag `v0.5.0` builds `44b0e61`.

- Two pull requests, each green before merging: #14 (global shortcut), #15 (Homebrew cask).
- Locally: 196 browser cases on Playwright 1.60 (cached browsers; hotspot), 32 unit tests, 18 release and cask script tests, 117 Rust tests, fmt, strict Clippy, `cargo deny` licenses and bans, `brew style` on the cask.
- [Main CI](https://github.com/Abulkosim/weir/actions/runs/40741654215) and the [tag build](https://github.com/Abulkosim/weir/actions/runs/40743106017) green on every job.
- All eight checksummed files matched; `gh attestation verify` passed for the DMG, Mac archive, EXE, MSI, `latest.json` and `SHA256SUMS.txt`; the real updater test rejected altered bytes and a wrong signed version and installed 0.5.0 for x86_64 and arm64.
- Homebrew: `weir` 0.5.0 pushed to `Abulkosim/homebrew-tap` by hand (`ecdc618`); `brew install --cask abulkosim/tap/weir` installed it on the owner's Mac, replacing 0.4.0, with history and settings kept; `brew livecheck` reports 0.5.0.
- Pending: pressing the recorded shortcut in the real app (no shortcut recorded on the owner's Mac yet), the shortcut on Windows, and the automatic tap bump (👤 deploy key, `docs/releasing.md`).

## v0.4.0

**[v0.4.0 is published](https://github.com/Abulkosim/weir/releases/tag/v0.4.0)** at 2026-10-26 04:19 UTC and marked latest; `latest.json` serves 0.4.0. Published on standing approval. Tag `v0.4.0` builds `4ec9bff`.

- Built as three pull requests, each green before merging: #10 (plan), #13 (report; replaces #11, closed when its base merged), #12 (scheduled tests, including the Windows `GetSystemPowerStatus` build).
- Locally: 194 browser cases on Playwright 1.60 (cached browsers; the dev Mac is on a hotspot), 29 unit tests, 14 release-script tests, 116 Rust tests, fmt, strict Clippy, `cargo deny` licenses and bans. The fit test flaked by one pixel in both engines; fixed in the app (fractional measuring), 96 of 96 repeated runs pass. A sample report was rendered and checked page by page.
- [Main CI](https://github.com/Abulkosim/weir/actions/runs/40266524882) and the [tag build](https://github.com/Abulkosim/weir/actions/runs/40267574661) green on every job.
- All eight checksummed files matched; `gh attestation verify` passed for the DMG, Mac archive, EXE, MSI, `latest.json` and `SHA256SUMS.txt`; the real updater test rejected altered bytes and a wrong signed version and installed 0.4.0 for x86_64 and arm64.
- Installed on the owner's Mac from the verified DMG, replacing 0.3.0; history and settings kept.
- Pending: a scheduled test firing on a real unmetered link (the dev Mac is on a hotspot, where scheduled tests correctly stay paused), the PDF opened in Preview from the real app, and Windows Battery Saver behaviour in a VM.

## v0.3.0

**[v0.3.0 is published](https://github.com/Abulkosim/weir/releases/tag/v0.3.0)** at 2026-10-25 02:31 UTC and marked latest; `latest.json` serves 0.3.0. Published on standing approval. Tag `v0.3.0` builds `d160695`.

- Pull request #9: browser tests on Playwright 1.64 (Chromium, WebKit), both installer builds and security checks green. Locally: 184 browser cases on Playwright 1.60 (cached browsers; the dev Mac is on a hotspot), 23 unit tests, 100 Rust tests, fmt, strict Clippy.
- [Main CI](https://github.com/Abulkosim/weir/actions/runs/40153927071) and the [tag build](https://github.com/Abulkosim/weir/actions/runs/40154327045) green on every job.
- All eight checksummed files matched; `gh attestation verify` passed for the DMG, Mac archive, EXE, MSI, `latest.json` and `SHA256SUMS.txt`; the real updater test rejected altered bytes and a wrong signed version and installed 0.3.0 for x86_64 and arm64.
- Installed on the owner's Mac from the verified DMG, replacing 0.2.0; history and settings kept.

## v0.2.0

**[v0.2.0 is published](https://github.com/Abulkosim/weir/releases/tag/v0.2.0)** at 2026-10-24 01:06 UTC and marked latest; `releases/latest/download/latest.json` serves 0.2.0. Published on standing approval after these checks. Tag `v0.2.0` builds `3342f39`.

- Local: `pnpm typecheck`, 21 unit tests, 178 Chromium/WebKit browser cases (axe on 10 views × 2 themes), 14 release-script tests, 99 Rust tests, `cargo fmt --check`, strict Clippy. `node scripts/check-release.mjs v0.2.0` passed.
- [Main CI](https://github.com/Abulkosim/weir/actions/runs/40045875308) and the [tag build](https://github.com/Abulkosim/weir/actions/runs/40046858843) green on every job, including Windows x64, which compiles and tests `IcmpSendEcho`, signal bars and the outage watch.
- All eight checksummed files matched `SHA256SUMS.txt`. `gh attestation verify` passed for the DMG, Mac archive, EXE, MSI, `latest.json` and `SHA256SUMS.txt`.
- Real updater test against the exact `Weir.app.tar.gz`: altered bytes and a mismatched signed version rejected; 0.2.0 installed for x86_64 and arm64. DMG app ad-hoc signed, version 0.2.0, universal.
- Live on this Mac (macOS 26.6.2, Wi-Fi): unprivileged ICMP reached the router 3 of 3; the connection check read Wi-Fi, router, DNS and Cloudflare as good ("Everything looks fine from here."). About 10 kB.
- Pending: a real outage caught by the watch (needs a deliberate disconnect with watching on), notifications on screen, the connection check on Windows (ICMP and signal bars), and the first in-app update from 0.1.0 to 0.2.0 on the owner's Mac.
- 2026-10-24: the owner reports checking the pending items by hand (outage watch and its notifications, the connection check on Windows, the in-app update from 0.1.0 to 0.2.0), with no problems reported.

## v0.1.0

**[v0.1.0 is published](https://github.com/Abulkosim/weir/releases/tag/v0.1.0)** at 2026-10-24 00:11 UTC and marked latest; `releases/latest/download/latest.json` serves 0.1.0. The changelog and release notes were re-dated to the publish day; the `latest.json` asset keeps its signed build-time notes and was not edited.

Verified 2026-10-23 on macOS 26.6.2 against the draft built from tag `v0.1.0` (`e53489b`).

- Local before tagging: `pnpm typecheck`, 18 unit tests, 138 Chromium/WebKit browser cases (incl. axe on 8 views × 2 themes), 14 release-script tests, 80 Rust tests, `cargo fmt --check`, strict Clippy. `node scripts/check-release.mjs v0.1.0` passed.
- [Main CI](https://github.com/Abulkosim/weir/actions/runs/40006765027) green on all jobs, including Windows x64. The [tag build](https://github.com/Abulkosim/weir/actions/runs/40010701946) built, signed and prepared the draft. Earlier main runs needed two fixes: the Ubuntu apt mirror stalled for 40 minutes (cancelled, re-run), and Linux WebKit needed the axe and fit checks to wait for a settled page.
- Draft assets: DMG, `Weir.app.tar.gz`, NSIS EXE, MSI, three `.sig` files, `latest.json`, `SHA256SUMS.txt`, SBOM. All eight checksummed files matched `SHA256SUMS.txt`.
- `gh attestation verify` passed for the DMG, Mac archive, EXE, MSI, `latest.json` and `SHA256SUMS.txt`.
- Real updater test against the exact `Weir.app.tar.gz`: altered bytes and a mismatched signed version were rejected; 0.1.0 installed into a disposable app for x86_64 and arm64.
- DMG app: ad-hoc signed (`codesign --verify --deep --strict` passes), identifier `com.abulkosim.weir`, version 0.1.0, minimum macOS 12.0, universal (x86_64, arm64).
- `latest.json`: version 0.1.0, platforms darwin-aarch64, darwin-x86_64, windows-x86_64 (NSIS fallback), -nsis, -msi; URLs under `Abulkosim/weir/releases/download/v0.1.0/`.
- Pending: launching the DMG build on the owner's Mac (Open Anyway path), Windows install and tray in a VM, a side-by-side comparison with speed.cloudflare.com on an unmetered link, and an in-app update (first possible with v0.1.1).

## Before v0.1.0

- 2026-10-23, macOS 26.6.2: the owner ran a real test on a phone hotspot from the dev build. Result (as shown in the popover): 18 Mbps down, varying 1.3–37 Mbps, 53 Mbps up, 142 ms idle latency, 537 ms under load, verdict "Calls will lag when the line is busy". It ran end to end through the real Cloudflare endpoints. Not yet compared with speed.cloudflare.com on an unmetered link (plan 1.6).
- 2026-10-23: the macOS path watcher reports `metered=Hotspot` on that hotspot (`cargo test prints_the_current_path -- --ignored`). No traffic sent.
- The owner confirmed the popover sizes to its content after the clipping fix (`e08fe23`).
