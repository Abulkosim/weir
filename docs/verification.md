# Verification

What was actually checked, newest release on top. Date, command, platform and outcome. No IP addresses, cities or Wi-Fi names.

## v0.1.0 (draft)

Pending. Filled in after the tag build.

## Before v0.1.0

- 2026-10-23, macOS 26.6.2: the owner ran a real test on a phone hotspot from the dev build. Result (as shown in the popover): 18 Mbps down, varying 1.3–37 Mbps, 53 Mbps up, 142 ms idle latency, 537 ms under load, verdict "Calls will lag when the line is busy". It ran end to end through the real Cloudflare endpoints. Not yet compared with speed.cloudflare.com on an unmetered link (plan 1.6).
- 2026-10-23: the macOS path watcher reports `metered=Hotspot` on that hotspot (`cargo test prints_the_current_path -- --ignored`). No traffic sent.
- The owner confirmed the popover sizes to its content after the clipping fix (`e08fe23`).
