# Privacy

Weir measures your internet connection against Cloudflare's speed test servers and keeps the results on your computer. No Weir account or hosted backend is involved.

This page describes Weir's current release. It is derived from the source code; see the files linked in each section.

## Summary

- No accounts, telemetry, analytics, crash reporting, or ads.
- Network requests go only to Cloudflare's speed test servers, to GitHub Releases for update checks, and, when you check your connection, to your own router and DNS server.
- Weir never runs a speed test on its own. Every test starts when you click **Test**. Outage watching, off by default, sends one tiny request a minute when you turn it on.
- History and settings stay on your computer. They never include your IP address, location, network provider, or Wi-Fi name.

## Network requests

All requests use HTTPS and send a `Weir/<version> (+https://github.com/Abulkosim/weir)` user agent ([`backends/mod.rs`](src-tauri/src/core/backends/mod.rs)).

| Host | Request | When | Sent |
| --- | --- | --- | --- |
| `speed.cloudflare.com` | `GET /cdn-cgi/trace` | Start of each test | Nothing about you |
| `speed.cloudflare.com` | `GET /__down?bytes=0` | About 20 latency probes per test, plus probes during download and upload | Nothing about you |
| `speed.cloudflare.com` | `GET /__down?bytes=N` | Download part of each test | Nothing about you |
| `speed.cloudflare.com` | `POST /__up` | Upload part of each test | Zero bytes as test data |
| `github.com` | `GET /Abulkosim/weir/releases/latest/download/latest.json` | Once a day while **Check for updates daily** is on (default), and when you choose **Check for updates** | Nothing about you or your tests |
| `github.com` | Download of the matching release asset | Only when you choose to install an update | Nothing about you or your tests |
| `speed.cloudflare.com` | `GET /__down?bytes=0`, twice on a new connection | When you choose **Check connection** | Nothing about you |
| `speed.cloudflare.com` | `GET /__down?bytes=0` on a kept-alive connection | About once a minute, only while **Watch for outages** is on (off by default) | Nothing about you |
| Your router | Five ICMP echo requests (or TCP connection attempts to ports 80, 443 and 53 if ICMP is not allowed) | When you choose **Check connection** | Nothing; it stays on your network |
| Your DNS server | Three lookups of `speed.cloudflare.com` over UDP | When you choose **Check connection** | The name `speed.cloudflare.com` |

Each test request carries a random number in its query string so caches don't answer it ([`cloudflare.rs`](src-tauri/src/core/backends/cloudflare.rs)). GitHub may redirect release files to its own download storage, such as `objects.githubusercontent.com`.

Weir does not send test results anywhere, including Cloudflare's own results endpoint. It does not contact any other host. The router address and DNS server come from your operating system and are used only for that check; they are not stored. Wi-Fi signal strength and link rate come from the operating system too, never the network name ([`probe.rs`](src-tauri/src/core/probe.rs), [`platform/`](src-tauri/src/platform)). The interface itself has no network access: its content security policy allows connections only to Weir's local process ([`tauri.conf.json`](src-tauri/tauri.conf.json)).

To tell whether you are online or on a metered connection, Weir asks the operating system (Network framework on macOS, connection profile on Windows). This sends no traffic.

The Windows installer may download Microsoft's WebView2 setup program if WebView2 is missing. That is part of installation, not the app.

## What Cloudflare sees

The test server belongs to Cloudflare. Like any website you visit, it sees your IP address and the times and sizes of the requests. Weir cannot hide that. See [Cloudflare's privacy policy](https://www.cloudflare.com/privacypolicy/).

Cloudflare's replies include your IP address, approximate location, and network provider. Weir reads only the data-centre code (for example `FRA`) from them and discards the rest. It is never stored, logged, or shown.

## Data use

A test downloads up to 380 MB and uploads up to 190 MB, about 600 MB at most with overhead. Faster connections use more. Weir shows an estimate before every test and the data used while it runs. See [docs/method.md](docs/method.md#data-use).

On a hotspot, a cellular link, Low Data Mode (macOS), or a connection Windows marks as metered, roaming, or over its data limit, Weir asks before testing: "You're on a hotspot. Test anyway?" ([`network.rs`](src-tauri/src/core/network.rs)). Weir has no scheduled or background tests.

## What Weir stores

Weir writes only to its own data folder:

| OS | Folder |
| --- | --- |
| macOS | `~/Library/Application Support/com.abulkosim.weir/` |
| Windows | `%APPDATA%\com.abulkosim.weir\` |

| File | Contents |
| --- | --- |
| `history.json` | Each test for up to one year or 5,000 tests: start time, duration, provider (`Cloudflare`), data-centre code, latency, jitter, download and upload speeds, latency under load, and data used. Failed tests keep the time and the reason. Outages keep their start and end times. |
| `settings.json` | Theme, tray speed text, launch at login, daily update checks, outage watching and its notifications |

Neither file holds an IP address, city, network provider (ASN), coordinates, Wi-Fi name (SSID), or router address. Files are replaced atomically. An unreadable file is moved aside as `history.unreadable-<time>.json` or `settings.unreadable-<time>.json`, never deleted. See [`history.rs`](src-tauri/src/core/history.rs) and [`settings.rs`](src-tauri/src/core/settings.rs).

**Export CSV** in Settings → History writes a file only where you choose, with the same fields. **Report** in History (or **Save PDF** in Settings → History) writes a PDF of the last 30 days the same way: the numbers, the plan speeds you entered, outages and failure reasons, and nothing else. Weir never uploads either file. **Clear history** deletes the entries after you confirm.

Other changes Weir makes, only when you turn them on:

- **Launch at login:** registers Weir with the OS. On macOS this is a LaunchAgent in `~/Library/LaunchAgents`. On Windows this is a per-user startup entry.
- **Updates:** replaces the installed app with the downloaded release.

## Updates

Weir checks for updates once a day, starting two minutes after launch, unless you turn off **Check for updates daily** in **Settings → Updates**. You can also check at any time. A background check only reads the manifest; it never downloads or installs. Weir installs only after you confirm.

- The manifest URL is fixed to the latest GitHub release of `Abulkosim/weir`.
- Downloads must be HTTPS on `github.com`, from this repository's release for that exact version ([`updates.rs`](src-tauri/src/updates.rs)).
- Each download must carry a valid minisign signature from the public key in [`tauri.conf.json`](src-tauri/tauri.conf.json), and the signed version must match the offered version. Otherwise nothing is installed.

## Installers

Installers are not signed by Apple or Microsoft yet. macOS builds are ad-hoc signed. See [Install](docs/install.md) for first-launch steps and how to verify a download against `SHA256SUMS.txt` and its build attestation.

## Removing your data

Quit Weir, delete the app, then delete the data folder listed above. Turn off launch at login first, or remove the LaunchAgent or startup entry by hand. See [Uninstall](docs/install.md#uninstall).

## Questions

Open an [issue](https://github.com/Abulkosim/weir/issues). For security problems, see [SECURITY.md](SECURITY.md).
