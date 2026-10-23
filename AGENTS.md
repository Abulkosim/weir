# Brief: internet speed and connection monitor in the tray (macOS + Windows)

You are building this from zero, as its owner-engineer. This file is the whole context. Read it once, then own the project.

The owner already shipped **Margin** (github.com/Abulkosim/margin), a Tauri tray app that shows Claude/Codex usage. They love how it looks, reads and ships. This app is Margin's sibling: same styling, same tone, same taste, same changelog and release discipline. Separate project, separate repo.

Clone Margin read-only for reference: `git clone https://github.com/Abulkosim/margin ~/Documents/margin-ref`. Read `src/styles.css`, `src/design/controls.css`, `src/components/ui/*`, `docs/releasing.md`, `docs/verification.md`, `docs/plan.md`, `changelogs/`, `CHANGELOG.md`, `.github/workflows/`, `scripts/`, `README.md`, `site/`. Copy patterns and files freely. Never push to Margin.

## What and why

fast.com in the tray, for ordinary people, not network engineers. Click the icon, see download, upload and latency in seconds. Then go further than fast.com: answer "is it me or the internet?" and keep a quiet history that shows drops and slow hours, with a report you could show your provider.

Reference products (study, don't clone): fast.com, speed.cloudflare.com, Speedtest by Ookla app, Google's speed test (M-Lab), macOS `networkQuality`, Waveform bufferbloat test, PingPlotter, iStat Menus network panel. Ours: one-click simple like fast.com, more honest about what the numbers mean, prettier than all of them.

## Step 1: research, then question the owner

Before writing code:

1. **Research (cite sources in `docs/research.md`).**
   - **Test backends.** For each, document endpoints, Terms of Service, license, data used, and what the operator publishes about your test:
     - Netflix fast.com (`api.fast.com`, token embedded in fast.com's JS). Unofficial; check ToS. Likely not acceptable as the default.
     - Cloudflare (`speed.cloudflare.com/__down?bytes=N`, `__up`, and the `@cloudflare/speedtest` package and methodology). Likely default.
     - M-Lab NDT7 (open protocol, used by Google's test). M-Lab publishes test results, including IP addresses, as open data. If used, say so in the UI before the first test.
     - Ookla Speedtest CLI. License likely forbids redistribution; verify and probably skip.
     - macOS `networkQuality` (Apple servers, responsiveness in RPM, latency under load). Mac-only bonus.
   - **Method.** Parallel streams, warm-up discard, test duration vs data used, idle latency vs loaded latency (bufferbloat), jitter, packet loss. How fast.com and Cloudflare compute their numbers. Pick a method and write it in `docs/method.md`.
   - **Data cost.** A full test can use hundreds of MB. How to detect metered or hotspot links: macOS `NWPath.isExpensive` / `isConstrained`, Windows `NetworkCostType`. Never auto-run a full test on one.
   - **"Is it me or the internet?" checks.** Wi-Fi signal and rate (macOS CoreWLAN; Windows WLAN API; check permissions, Location may be required for SSID on recent macOS), router reachability (gateway ping), DNS resolution time, external reachability. Which need elevated rights. ICMP without root: check macOS unprivileged `SOCK_DGRAM` ICMP and Windows `IcmpSendEcho`; fall back to TCP connect timing.
   - **Competitors' complaints:** ads, accounts, inflated numbers, battery drain, data use.
2. **Question the owner** in one short batch (≤10 questions, multiple choice where possible, your recommendation first). Ask only what changes the build. Suggested:
   - Default backend (recommend Cloudflare), and whether to offer M-Lab as an option.
   - Background monitoring: off, light only (latency checks every minute, no big downloads), or scheduled full tests (e.g. every 6h, never on metered).
   - Tray shows: last download speed as text, a glyph only, or connection health.
   - History length and an exportable report (PDF/CSV) for v0.1 or later.
   - Same sage accent as Margin, or a sibling hue on the same neutrals.
   - Name pick from your 3 proposals.
   - Commit style: Margin uses one-line imperative subjects; the owner's global default is Conventional Commits with a body. Which here?
3. **Write `docs/plan.md`** (format below) from research + answers. Then build.

Decide everything else yourself. Don't ask the owner to pick between things you can decide.

## Decisions already made (don't relitigate)

- **Stack:** Tauri 2 + Rust core + Vue 3 + TypeScript + Vite, pnpm, Node ≥24. Reka UI for headless controls, Zod for validating every IPC payload. Same as Margin, so the design system ports directly. Plugins: tray, autostart, notification, updater, store (or own JSON), single-instance, positioner.
- **macOS and Windows** from one codebase, like Margin. Windows built in CI; the owner tests it in a VM later.
- **Rust owns all network and measurement.** Frontend sees results and progress only.
- **Respect data.** Show estimated data use per test. Full tests are manual by default. Never run automatic full tests on metered, hotspot or low-data connections. Background checks must stay tiny (a few KB per minute at most).
- **Honest numbers.** Show what was measured, against which server, when. No inflated "peak" numbers. If a test fails or is cut short, say so; never show a partial number as a result.
- **Privacy.** No accounts, no ads, no telemetry, no analytics. History stays local. Only our test servers and the GitHub update check are contacted. Never put the user's IP, SSID or location in screenshots, logs or fixtures.
- **Free only.** No paid signing, no notarization, no paid services or paid APIs. GitHub Actions, Releases, Pages only.
- **Distribution:** GitHub Releases, ad-hoc signed on mac, unsigned by a verified publisher. Document right-click → Open and SmartScreen "Run anyway". Tauri updater signatures (minisign) from day one.
- **License:** MIT. Author `abulkosim <abulkasimkattaev@gmail.com>`. Repo under `Abulkosim/<name>`.
- **Tone:** it measures your connection. It is not a "boost your internet" tool, and never claims to speed anything up.

## Name

Propose 3: short, memorable, one word, not "-ify", not "SpeedX", not a fast.com or Speedtest lookalike. Check GitHub, Homebrew casks, winget and app stores for collisions. Pick one unless the owner objects.

## Design: same family as Margin

Margin's look is the spec. Port it, don't reinvent it. Use the tokens in Margin's `src/styles.css` verbatim (light and dark), unless the owner picks a new accent.

- Inter, 14px base, tight hierarchy, `font-variant-numeric: tabular-nums` on every number. Speeds count up smoothly to their value during a test, then settle.
- One accent. No borders where spacing or shadow does the job. Radii: popover 12px on mac, 8px on Windows; cards 10px; controls 6–8px.
- Motion 150–250ms, `ease`, never bouncy. The running test is a calm live gauge or line, not a spinner. Respect `prefers-reduced-motion`.
- Dark and light both first-class. Follow OS, switchable in Settings.
- macOS popover: translucent/vibrancy, frameless, anchored to the tray icon, hides on blur. Windows: Mica/acrylic on Win11, else solid, anchored to tray, correct DPI at 125/150/200%.
- The popover **sizes to its content** (Margin's `fit_popover` command + ResizeObserver; reuse it) and opens straight on the result and the Test button. No title block eating the top. Margin learned this in v0.5.1.
- Numbers need plain meaning: "Good for 4K video and calls", "Calls may stutter: latency jumps to 180 ms under load". Units always shown (Mbps, ms).
- Visible focus ring (`--focus`, 2px, offset 3px). Full keyboard use. Screen-reader labels.
- Designed states: never tested, testing, result, failed, offline, metered (test needs confirmation), stale result, Wi-Fi weak, router unreachable, DNS slow.
- Settings get the same care as the popover. Margin's Settings structure is the model.
- Bar: a 2026 indie app a designer shipped. Raycast, Linear, Arc, CleanShot, Ice. If a popover screenshot wouldn't get upvotes alone, it isn't done.

**Tray icon:** monochrome template on mac (alpha only, adapts to menu bar), colored only if needed on Windows, legible at 16px on light and dark taskbars. Same metaphor on both. Candidates: a small gauge or ring that fills while testing and shows health at rest; optional speed text beside it on mac. Decide and justify in one line.

**Build the smallest version, screenshot light + dark, look at it, iterate.**

## Copy

- Short, practical, plain. Name the action or the information. No slogans, no filler, no exclamation marks, no cute.
- Margin examples: "Usage remaining" (not "Your headroom"), "Resets in 2h 18m · 16:30", "Saves automatically", "No accounts connected.", "Check for updates daily".
- For this app: "Download 312 Mbps", "Upload 41 Mbps", "Latency 14 ms · 62 ms under load", "Tested 12 min ago · Cloudflare, Frankfurt", "Uses about 250 MB", "You're on a hotspot. Test anyway?", "Your Wi-Fi is fine. The problem is past your router.", "No internet since 14:02".
- Notifications are terse, one line, and off by default except "connection lost/back" if the owner agrees.

## Feature direction (validate with research; don't build all at once)

Core (v0.1): one-click test from the tray (download, upload, idle and loaded latency, jitter); server and time shown; data estimate; metered guard; last result in the popover; history list; launch at login.

Likely next: light background checks and an outage timeline ("No internet 14:02–14:09"); "is it me or the internet?" diagnosis (Wi-Fi → router → DNS → internet), each step a plain sentence; slow-hours chart; scheduled full tests; exportable report for the provider; "what this speed is good for" guidance.

Maybe: compare to the speed you pay for; per-network history (home vs office); global shortcut; `--json` CLI.

## Repo layout (adjust freely)

```
src-tauri/src/core/{speedtest,backends,latency,diagnose,metered,history}.rs
src-tauri/src/{tray,commands}.rs
src/{App.vue, styles.css, design/, components/, components/ui/, components/settings/, lib/}
tests/unit, tests/e2e
changelogs/, CHANGELOG.md, docs/{plan,research,method,verification,releasing,install}.md
scripts/ (release checks, manifest, checksums: port Margin's)
site/ (GitHub Pages landing, same template as Margin's)
```

- Measurement math (throughput from samples, warm-up discard, percentiles, jitter, outage detection) in pure functions with unit tests on recorded sample fixtures.
- Backends behind a trait; test them against a local mock server (wiremock or similar) for success, slow, stalled, cut-off and error cases. No real network in tests.
- A demo bridge like Margin's (`?demo=1&state=…`) so every UI state renders in a plain browser for Playwright (Chromium + WebKit) and screenshots. One demo state per designed state, including a running test.
- Settings: versioned JSON, tolerant per-field loading, recovery notes, back up before reset. History in its own bounded file.

## Workflow

- **`docs/plan.md` is the work queue.** Phases end in ⛳ release boundaries. Each task: one line of what, one **Verify:** line. Tick when verified; write findings under it. Owner-only items marked 👤: prepare everything, leave unchecked, never act for the owner (posting, account settings, Windows VM testing).
- **One task per iteration**, smallest shippable slice. If a task turns out wrong or blocked, write why under it and move on.
- **Before every commit, all green:** `pnpm typecheck && pnpm test && pnpm test:e2e`, then `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` in `src-tauri`.
- **UI change:** screenshot light + dark through Playwright, look at it, refresh `docs/images` if visible copy changed. Write screenshots to the test's output path, never a hard-coded temp path (that broke Margin's CI).
- **Commits:** per the owner's answer above. Either way: imperative, no AI attribution, no `Co-Authored-By`, no trailers. Commit as `abulkosim <abulkasimkattaev@gmail.com>`. Push to `main` after each verified task.
- **Code:** no comments unless non-obvious. Lockfiles committed; `--frozen-lockfile` and `--locked` in CI.
- **Real-network checks** happen manually on the owner's Mac, sparingly, never on a metered link. Record results in `docs/verification.md` without IPs or SSIDs.
- **Reporting:** extremely concise. Short lines. Show screenshots. Ask only real decisions. End with what's done and what the owner must do.

## Releases (exactly like Margin)

Port Margin's `docs/releasing.md`, `scripts/` and `.github/workflows/`, and rename.

- **Version** in `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, checked by a script.
- **One `changelogs/vX.Y.Z.md` per release**, linked from `CHANGELOG.md`. Format:

  ```md
  # v0.5.0

  2026-10-22 · [Download](https://github.com/Abulkosim/<name>/releases/tag/v0.5.0)

  - The tray icon follows the session and weekly limits by default. Change it in Settings → Tray shows.
  - Each limit shows the reset time next to the countdown, for example "Resets in 2h 18m · 16:30".
  ```

  A few concrete, user-facing bullets, checked against git history. Say where the setting lives. No implementation detail. Date = actual publish date (UTC); fix it if the draft was dated earlier.
- **`CHANGELOG.md`** is a table: `| Version | Date (UTC) | Changes |` with a one-phrase summary per row.
- GitHub release notes and updater notes come from that changelog file. Test logs and evidence go in `docs/verification.md` (newest release on top: what passed, counts, CI run links, checksums, updater test, what's still pending).
- **CI** (`ci.yml`): Ubuntu 24.04 frontend + Playwright (timeout 45 min; the apt mirror stalls), macOS (fmt, clippy, tests, universal DMG + `.app.tar.gz`), Windows x64 (fmt, clippy, tests, NSIS + MSI). Actions pinned by SHA. Least-privilege permissions. Concurrency cancels superseded `main` runs, never tags. Dependabot. Plus `security.yml` (cargo audit/deny, pnpm audit) and `scorecard.yml`.
- **Tag release:** signs updater artifacts, validates versions/lockfiles, generates `latest.json`, `SHA256SUMS.txt`, SPDX SBOM, build provenance attestations, creates a **draft** release.
- **Release flow:** build → tests → CI green on `main` → annotated tag → CI builds draft → download draft, verify checksums, run the real updater install test against the exact archive (tampered bytes and wrong signed version must be rejected) → record in `docs/verification.md` → report to owner → **owner says "publish"** → publish, mark latest → owner updates their installed app in-app. Never edit a published release's files or reuse a version; ship a new patch.
- Generate the updater key with `pnpm tauri signer generate`, store outside the repo with a backup, put it in the `TAURI_SIGNING_PRIVATE_KEY` secret. Losing it strands every install.
- "Small patch" without a release = commit + push only.
- Later, gated off by default: Homebrew tap and winget jobs (Margin has both in `packaging/`).

## Done means

- M1 Alive: tray icon, popover, one-click test against the default backend with live progress and a final result that matches speed.cloudflare.com within a reasonable margin on the same machine. Screenshot.
- M2 Real: chosen v0.1 features, metered guard, history, settings, launch at login, every state designed and in the demo bridge.
- M3 Ship: Windows build green in CI, README (hero screenshots, 3-bullet value, Install in the first screen, comparison table vs fast.com / Ookla / Cloudflare / `networkQuality`), install notes for unsigned builds, Pages site, v0.1.0 published through the full release flow.
- M4+: whatever the plan says next.

Start with research and the question batch. Keep the owner posted in short lines.
