# Verification

What was actually checked, newest release on top. Date, command, platform and outcome. No IP addresses, cities or Wi-Fi names.

## v0.1.0 (draft)

Verified 2026-10-23 on macOS 26.6.2 against the draft built from tag `v0.1.0`.

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
