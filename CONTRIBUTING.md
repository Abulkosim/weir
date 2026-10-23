# Contributing

Weir tests internet speed from the menu bar or system tray. Bugs and requests go to [issues](https://github.com/Abulkosim/weir/issues/new/choose); vulnerabilities go to [SECURITY.md](SECURITY.md).

## Scope and rules

- Weir measures a connection. Never describe it as a way to speed one up.
- All network access and measurement live in Rust. The interface receives results and progress only.
- Honest numbers: report the sustained speed, not a peak. A test that fails or is cut short shows no number for that metric and says why. The method is in [docs/method.md](docs/method.md).
- Tests run only when the user asks. On a metered connection, Weir asks first and shows the data estimate.
- Never store, log, or show the IP address, city, network provider (ASN), coordinates, Wi-Fi name, or router address. Of Cloudflare's response, keep only the data-centre code.
- No accounts, telemetry, analytics, or crash reporting. Network hosts are listed in [PRIVACY.md](PRIVACY.md#network-requests); adding one needs a strong reason and a PRIVACY.md update.

## Prerequisites

- Node.js 24 and pnpm 11.25.0 (pinned in `package.json`).
- Stable Rust via [rustup](https://rustup.rs), 1.90 or newer, with `clippy` and `rustfmt`.
- [Tauri's platform prerequisites](https://v2.tauri.app/start/prerequisites/). macOS: Xcode Command Line Tools. Windows: Microsoft C++ Build Tools (Desktop development with C++) and WebView2.
- Playwright browsers for e2e tests: `pnpm exec playwright install chromium webkit`.

## Run

```sh
pnpm install --frozen-lockfile
pnpm tauri dev
```

The desktop app runs real tests against Cloudflare. A test can use up to 600 MB, so avoid metered connections while developing. To work on the interface without network requests, use the browser preview:

```sh
pnpm dev
```

Open `http://127.0.0.1:1430/?demo=1`. Sample data is labeled as such. Add `&theme=light` or `&theme=dark`, `&page=history` or `&page=settings`, and `&state=` one of:

| State | Shows |
| --- | --- |
| `never` | Never tested; press Test to run a simulated test |
| `result` | Last result with its meaning (default) |
| `testing-latency` | Test running, latency step |
| `testing` | Test running, download step |
| `testing-upload` | Test running, upload step |
| `stutter` | Result where calls may stutter under load |
| `partial` | Download measured, upload and latency not measured |
| `varied` | Download that varied, shown with its range |
| `capped` | Download that hit the data cap |
| `stale` | Result older than a day |
| `failed` | Failed test; add `&error=` `offline`, `refused`, `serverError`, `stalled`, `tooShort`, `dropped`, `networkChanged`, or `cancelled` |
| `metered` | On a hotspot; Test asks first |
| `offline` | No internet since 14:02 |
| `design` | Every control in both themes |

States are defined in [`src/lib/demo.ts`](src/lib/demo.ts).

## Test

```sh
pnpm typecheck && pnpm test && pnpm test:e2e
cd src-tauri
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Run `pnpm build` once before the Rust checks; they need the frontend assets. CI runs the same checks with `--locked` ([`ci.yml`](.github/workflows/ci.yml)).

Measurement math is pure and tested on recorded sample fixtures ([`src-tauri/fixtures`](src-tauri/fixtures/README.md)). Backends are tested against a local mock server. Tests never touch the real network. A measurement change needs a fixture and a regression test.

Never commit IP addresses, locations, Wi-Fi names, or real `history.json` files.

## Layout

```
src-tauri/src/
  commands.rs          Tauri command surface
  runtime.rs           test state and events to the popover
  updates.rs           update checks and install on request
  platform/            macOS and Windows network monitors
  core/
    backends/          backend trait and Cloudflare client
    speedtest.rs       test sequence, progress, cancel
    stats.rs           throughput, latency, jitter, validity
    meaning.rs         health and plain-meaning lines
    network.rs         online and metered state
    history.rs         local test history
    export.rs          CSV export
    settings.rs        local settings file
    tray.rs, ring.rs   tray menu and icon rendering
src-tauri/fixtures/    throughput sample fixtures
src/
  App.vue, components/ popover, testing, history, settings
  design/, styles.css  tokens and controls
  lib/                 state, formatting, demo data
tests/unit/, tests/e2e/
```

## Commits and pull requests

- [Conventional Commits](https://www.conventionalcommits.org) with a body: `type(scope): summary`, imperative and lowercase, then a body that says what changed and why. For example `feat(history): export history as CSV`. No trailers.
- Keep pull requests focused. Fill in the template checklist.
- User-facing changes add a short bullet to the next `changelogs/vX.Y.Z.md`.
- UI changes include light and dark screenshots from the preview, with sample data.
- Copy is short and plain, with units: "Download 312 Mbps", "Uses about 250 MB". No slogans or exclamation marks.

## Releases

The maintainer tags releases. See [docs/releasing.md](docs/releasing.md).

## Conduct

See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
