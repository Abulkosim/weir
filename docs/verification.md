# Verification

What was actually checked, newest release on top. Date, command, platform and outcome. No IP addresses, cities or Wi-Fi names.

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
