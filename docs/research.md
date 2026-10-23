# Research

Done 2026-10-23, before any code. Sources were fetched that day. Tags: **[src]** read in the cited source, **[obs]** observed with curl from a dev Mac (no IPs, cities or SSIDs recorded), **[derived]** our arithmetic, **[unverified]** secondary source or not confirmed.

## Summary

- **Backend:** Cloudflare (`speed.cloudflare.com/__down`, `__up`) is the default. No ToS forbids third-party clients; we send an identifying User-Agent, cap bytes, never POST results, back off on 403/429. fast.com is out (unofficial token, Netflix ToS §1.8 bans automated means). Ookla CLI is out (EULA forbids redistribution and third-party use). M-Lab NDT7 publishes IPs as open data and needs consent; not planned. macOS `networkQuality` is a possible Mac-only second opinion later.
- **Method:** parallel HTTP/1.1 streams, warm-up discarded by ramp detection, steady-window mean (never a peak), loaded latency on a separate connection, hard caps. Full method in [method.md](method.md).
- **Data:** a test costs ~110 MB at 100/20 Mbps and ~500 MB at 1 Gbps; hard ceiling 600 MB. Metered detection: macOS `nw_path_is_expensive` / `nw_path_is_constrained`, Windows `ConnectionCost.NetworkCostType` (Unknown treated as metered).
- **Diagnosis:** Wi-Fi RSSI, noise and rate work on macOS without Location; SSID does not, and we never read it. Windows 24H2 gates the whole WLAN connection query behind Location, so use signal bars and link speed. Gateway via `netdev`. ICMP works unprivileged on macOS (`SOCK_DGRAM`) and via `IcmpSendEcho2` on Windows; TCP connect timing as fallback.
- **Complaints to answer:** ads and trackers, accounts for history, battery drain, surprise data use, numbers that differ between sites, tests that look fine while calls lag (bufferbloat), numbers with no meaning, subscriptions.
- **Name:** Weir. A weir is a low wall across a stream that steadies its flow and is used to measure it.

## Speed test backends: research

Researched 2026-10-23. Sources fetched that day. "Observed" = probed from a dev Mac with curl on 2026-10-23 (no IPs/locations recorded here). "Unverified" = could not confirm from a primary source.

---

### 1. Cloudflare (speed.cloudflare.com)

#### Endpoints

| Endpoint | What | Notes |
| --- | --- | --- |
| `GET https://speed.cloudflare.com/__down?bytes=N` | Returns N bytes (`application/octet-stream`), `Cache-Control: no-store, no-transform` | `bytes=0` = latency probe. Observed: `N < 1e8` → 200; `N ≥ 1e8` → **403, body `0`**, unless a `Referer: https://speed.cloudflare.com/` header is sent (then 200). Undocumented. |
| `POST https://speed.cloudflare.com/__up` | Accepts body, returns 200 empty | Observed 200 for small and 30 MB bodies. |
| `GET https://h3.speed.cloudflare.com/__down?bytes=N` / `__up` | Same Worker on an HTTP/2+3 host | Observed `bytes=1e10` → HTTP/2 200 without Referer. This is the host Cloudflare's own Rust tool uses ([networkquality-rs `src/args/rpm.rs`](https://github.com/cloudflare/networkquality-rs/blob/main/src/args/rpm.rs): default large URL `https://h3.speed.cloudflare.com/__down?bytes=10000000000`). |
| `GET https://speed.cloudflare.com/cdn-cgi/trace` | Plain text `key=value`: `colo=`, `loc=` (country), `http=`, `tls=`, `ip=`, `uag=` … | Standard Cloudflare trace. Use `colo` for server, never store `ip`. |
| `GET https://speed.cloudflare.com/meta`, `/locations` | JSON used by the web UI | Observed **403 `{}`** without a speed.cloudflare.com Referer. Don't depend on them. |

Response headers on `__down` / `__up` (observed):

- `cf-meta-colo` / `colo` (IATA code of the serving data center, e.g. `FRA`), `CF-RAY` suffix (same IATA code).
- `cf-meta-ip`, `cf-meta-asn`, `cf-meta-country`, `cf-meta-city`, `cf-meta-latitude`, `cf-meta-longitude`, `cf-meta-postalCode`, `cf-meta-timezone`, plus bare `asn`, `city`, `country`, `latitude`, `longitude`. **These are the user's IP and geolocation: never log or persist them.** Only `colo` is needed.
- `Server-Timing: cfSpeedEdge;dur=3, cfSpeedWorker;dur=20` (server-side processing ms, to subtract from TTFB) and a second `server-timing: cfL4;desc="?proto=TCP&rtt=…&min_rtt=…&rtt_var=…&lost=…&retrans=…&cwnd=…"` (server's TCP_INFO view, RTT in µs). The engine sums all `cfSpeed*;dur=` values ([`cfGetServerTime`, speedtest.js](https://unpkg.com/@cloudflare/speedtest@1.15.0/dist/speedtest.js)).
- `access-control-expose-headers` lists the above; `timing-allow-origin: *`.

Colo → city: ship a static IATA → city table (or show the code). `/locations` is gated.

#### `@cloudflare/speedtest` npm package

- Version 1.15.0, **MIT** ("Copyright (c) 2023 Cloudflare"), repo [github.com/cloudflare/speedtest](https://github.com/cloudflare/speedtest), npm [@cloudflare/speedtest](https://www.npmjs.com/package/@cloudflare/speedtest). Browser-only (uses `PerformanceResourceTiming`), so we port the method to Rust, not the code.
- README: "Please note that measurement results are collected by Cloudflare on completion for the purpose of calculating aggregated insights regarding Internet connection quality."
- Default measurement sequence (from `dist/speedtest.js` `defaultConfig`; README shows a slightly older list without the interleaved latency steps):

  ```
  latency ×2
  download 100 kB ×1 (bypassMinDuration)
  latency ×20
  download 100 kB ×9   | latency ×2 between every set below
  download 1 MB ×8
  upload   100 kB ×8
  upload   1 MB ×6
  download 10 MB ×6
  upload   10 MB ×4
  download 25 MB ×4
  upload   25 MB ×4
  download 100 MB ×3
  upload   50 MB ×3
  download 250 MB ×2
  ```

- **Requests are strictly sequential, one at a time** (single in-flight request; `#nextMeasurement` chains on completion). No parallel streams.
- **Ramp stop:** after a set completes, if the *minimum* request duration in that set > `bandwidthFinishRequestDuration` (1000 ms), that direction is finished and larger sets in that direction are skipped.
- **Per-request bandwidth:** download `bps = 8·transferSize / (ping + payloadDownloadTime)`; upload `bps = 8·bytes·1.005 / ttfb`. `ping = ttfb − serverTime` (serverTime from `Server-Timing`).
- **Final download/upload:** **90th percentile** (`bandwidthPercentile: 0.9`, linear interpolation) of all per-request bps where `duration ≥ bandwidthMinRequestDuration` (10 ms).
- **Idle latency:** **median** (`latencyPercentile: 0.5`) of `bytes=0` request pings (TTFB minus server time).
- **Jitter:** mean absolute difference between consecutive latency samples: `Σ|p[i]−p[i−1]| / (n−1)`.
- **Loaded latency:** while a download/upload set runs, a parallel `bytes=0` probe every `loadedLatencyThrottle` = 400 ms. Only sets whose shortest request ≥ `loadedRequestMinDuration` (250 ms) count; keep the last `loadedLatencyMaxPoints` = 20 points; report the median. Reported separately for download and upload.
- **Packet loss:** opt-in, needs your own TURN server credentials (WebRTC/UDP). Not in defaults.
- **429:** engine retries up to 3 times, waiting `Retry-After` (default 5 s), then fails. No published rate limits.
- **Results upload:** on finish, POSTs a JSON summary to **`https://speed.cloudflare.com/__results`** (`logAimApiUrl`): latency points, per-request bps, loaded latency points, AIM scores, duration. A Rust client that calls `__down`/`__up` directly sends nothing unless it chooses to. We won't.

#### AIM scores

Defined in [developers.cloudflare.com/speed/aim](https://developers.cloudflare.com/speed/aim/) and [blog: AIM database for Internet quality (2023-04-18)](https://blog.cloudflare.com/aim-database-for-internet-quality/). Point tables from `internalConfig` in speedtest.js:

| Metric | Bands → points |
| --- | --- |
| Idle latency (ms) | <10:20, <20:10, <50:5, <100:0, <500:−10, else −20 |
| Loaded latency increase (max(down,up loaded) − idle, ms) | same as latency |
| Jitter (ms) | <10:10, <20:5, <100:0, <500:−10, else −20 |
| Packet loss | <1%:10, <5%:5, <25%:0, <50%:−10, else −20 (missing = 0) |
| Download / upload | <1 Mbps:0, <10:5, <50:10, <100:20, else 30 |

Use cases: **Streaming** = latency + loss + download + loaded increase (thresholds 15/20/40/60); **Gaming** = latency + loss + loaded increase (5/15/25/30); **Video calls (rtc)** = latency + jitter + loss + loaded increase (5/15/25/40). Classes: bad, poor, average, good, great. The blog: point values "are not fixed and may change". Useful as a starting point for "Good for 4K video and calls" copy; write our own bands in `method.md`.

#### Methodology statements

- [speed.cloudflare.com/about](https://speed.cloudflare.com/about): "Speed is measured by downloading/uploading progressively larger files and taking the 90th percentile speed." Ping = time before the browser "actually starts receiving data on a request". Jitter = "the average ping delta between any two consecutive measurements."
- [Blog: Test your home network performance (2020)](https://blog.cloudflare.com/test-your-home-network-performance/): p90 chosen to discount "peak outliers"; Cloudflare "does not sell your data".

#### Terms, privacy, what Cloudflare learns

- Speed test page (quoted from [speed.cloudflare.com](https://speed.cloudflare.com/)): "When you use Speed Test, Cloudflare receives the IP address you use to connect… uses your IP address to estimate your geolocation (at the country and city levels) and to identify the Autonomous System Number (ASN)… Cloudflare shares anonymized measurement information (e.g., the estimated geolocation, ASN associated with your Speed Test, etc.) with our measurement partners… We do not share your IP address with our measurement partners… Cloudflare truncates your IP address… to /24 and /48 for IPv4 and IPv6." Also: "Note: A speed test can consume up to 200MB of data."
- Measurement partner = M-Lab: [M-Lab is now providing Cloudflare AIM data for open access (2023-04-18)](https://www.measurementlab.net/blog/cloudflare-aimscoredata-announcement/). That applies to results posted to `__results`; direct `__down`/`__up` calls only leave ordinary request logs (IP, timing). Retention of those logs: unverified.
- [Website and Online Services Terms of Use](https://www.cloudflare.com/website-terms/) cover "products and services that are publicly available without a subscription or a Cloudflare account". Relevant: §7 "You may not exceed or circumvent, or try to exceed or circumvent, limitations on the Websites or Online Services" and no use that "could damage, disable, overburden, disrupt or impair any Cloudflare servers or APIs". §8: "You may not use automated bots to access, scan, scrape, data mine, copy, or use the materials" (framed as an AI/scraping restriction).
- **No explicit permission or prohibition for third-party clients of `__down`/`__up` found.** Signals that it's tolerated: MIT engine with configurable endpoints, `Access-Control-Allow-Origin: *`, Cloudflare's own BSD-3 Rust CLI `mach` hits `h3.speed.cloudflare.com` from outside the browser, and many third-party clients exist. Signals of limits: Referer gate on ≥100 MB on the main host, gated `/meta`. Risk: Cloudflare can change or block it without notice. Mitigations: identifying `User-Agent` (`<AppName>/<version> (+repo URL)`), stay within the default ladder, no automatic full tests, back off on 403/429, keep the backend behind a trait.

#### Data per test

- Engine ladder maximum: download 969 MB + upload 297 MB ≈ **1.27 GB** if every set finishes in < 1 s (multi-gigabit). Computed from the sequence above.
- Typical: ~100 Mbps down / 20 Mbps up stops after the 25 MB download set and 10 MB upload set ≈ 170 MB + 47 MB ≈ **~215 MB** (computed). Cloudflare's page says "up to 200MB".
- We control the ladder, so we can cap by bytes and time.

#### Related: Cloudflare `networkquality-rs` (`mach`)

- [github.com/cloudflare/networkquality-rs](https://github.com/cloudflare/networkquality-rs), crate `cf-mach` 0.4.0, **BSD-3-Clause**, Rust/Tokio, implements the IETF responsiveness draft against Cloudflare (`h3.speed.cloudflare.com`). Defaults: 12 s test, up to 16 load connections, 1 s intervals, MAD 4, 5% stability, 95% trimmed mean, 100 MB upload requests. README: "RPM reports are automatically uploaded to Cloudflare's aim database and are anonymous… Use `--disable-aim-scores` to disable." Upload target `https://aim.cloudflare.com/__log`. Library API "is not stable"; uses BoringSSL (needs CMake/Clang, NASM on Windows). Good reference implementation for parallel-stream + loaded latency in Rust; probably not a dependency.

---

### 2. Netflix fast.com

#### Endpoints (observed in `https://fast.com/app-0bffe1.js`)

- `GET https://api.fast.com/netflix/speedtest/v2?https=true&token=<T>&urlCount=5` → `{ client: { ip, asn, location }, targets: [{ url, location: { city, country } } ×5] }`. Targets are Netflix Open Connect appliances (`https://…oca.nflxvideo.net/speedtest?...`), often inside the ISP.
- `<T>` is a **static string hard-coded in the JS** (base64 of a keyboard-mash string), not per-user. Third-party tools scrape it from the JS ([npm `fast.com`](https://npmjs.com/package/fast.com), [Go `faster`](https://pkg.go.dev/github.com/yavosh/faster) notes it can rotate).
- Download: `…/speedtest/range/0-<N>` byte ranges from the OCA URLs. Client logs events to `https://ichnaea-web.netflix.com/cl2`.

#### Method (from the JS)

- Parallel connections: min 1, max 8; `maxBytesInFlight` 78,643,200 (75 MB).
- Duration: min 5–7 s, max 30 s; stops early when the speed is stable (`stableDeltaMeasurementsStopper`: stabilityDelta 2%, ≥ 6 stable measurements).
- Speed: moving average of the last 5 byte/time snapshots (`stableMovingAverage(5)`), i.e. a sustained, not peak, rate.
- Unloaded latency: per target 10th percentile, then min across targets. Loaded latency ("bufferbloat"): per target 75th percentile (only targets with > 4 samples), then min across targets. Upload loaded latency off by default.

#### Terms and privacy

- No public API, no developer terms, no permission for third-party clients found. The endpoint and token are undocumented internals.
- [Netflix Terms of Use](https://help.netflix.com/legal/termsofuse) §1.8: "use any robot, spider, scraper or other automated means to access the Netflix service"; "use any data mining, data gathering or extraction method". Whether fast.com is "the Netflix service" (defined as the service "for discovering and accessing Netflix content") is unclear; the [FAST.com privacy policy](https://help.netflix.com/legal/fastprivacy) footer links to these terms.
- FAST.com privacy (updated 2021-03-16) collects "IP address (which may tell us your general location and internet service provider (ISP))", "connection information such as speed and latency measurements", device identifiers and characteristics; "We do not sell personal information."

#### Data per test

Speed × duration, ≤ 30 s, up to 75 MB in flight. Example (computed): 300 Mbps for ~15 s ≈ 560 MB down, plus upload if run. No official figure found.

**Verdict:** unofficial, token scraped from JS, terms lean against automated access. Not acceptable as a default or option for a shipped app.

---

### 3. M-Lab NDT7

#### Protocol ([ndt7 spec](https://github.com/m-lab/ndt-server/blob/main/spec/ndt7-protocol.md))

- WebSocket, subprotocol `net.measurementlab.ndt.v7`. Paths `/ndt/v7/download`, `/ndt/v7/upload`. "Ndt7 always uses a single TCP connection."
- Each direction runs "up to ten seconds"; server may close at 13 s.
- Binary messages 2^10 to 2^24 bytes, start at 2^13, doubled for fast clients.
- Text `Measurement` JSON messages: `AppInfo { ElapsedTime, NumBytes }`, `TCPInfo` (kernel TCP_INFO: `MinRTT`, `RTT` (smoothed) in µs, `BytesAcked`, …), `ConnectionInfo { Client, Server, UUID }`. Speed = bytes / elapsed. MinRTT/RTT give latency under load from the server's kernel.
- Query string metadata optional, ≤ 4096 bytes; client "SHOULD also include a meaningful, non-generic User-Agent string". Convention: `client_name`, `client_version` ([ndt7-js README](https://cdn.jsdelivr.net/npm/@m-lab/ndt7@0.1.4/README.md)).
- Non-interactive clients: randomized schedule, exponential with 6 h mean, clamped 2160–54000 s.

#### Server discovery ([Locate v2](https://www.measurementlab.net/develop/locate-v2), [USAGE](https://github.com/m-lab/locate/blob/main/USAGE.md))

- `GET https://locate.measurementlab.net/v2/nearest/ndt/ndt7?client_name=…&client_version=…` → `results[]` (observed 4) with `machine`, `location { city, country }`, `urls` keyed `wss:///ndt/v7/download` etc., each with `?access_token=…`. "An invalid access token will always be rejected." Tokens required since 2021 ([blog](https://www.measurementlab.net/blog/why-access-tokens/)). 204 = no capacity. Token lifetime: unverified (short-lived, use immediately).
- Priority endpoint `v2/priority/nearest` with integrator JWT for registered integrations (ndt7-js README); registration process unverified.

#### Policies

- [Privacy policy](https://www.measurementlab.net/privacy/): connection data includes "the Internet Protocol (IP) address of the client that conducted the measurement"; "Your test data is made public"; published data "includes your IP address, date and time"; "All experiment data is retained indefinitely and published publicly"; data released into the public domain. Third-party client developers "must comply with this privacy policy".
- [Acceptable Use Policy](https://www.measurementlab.net/aup/): "Interactive users must not run more than 40 tests per day"; "Automated software or hardware clients should not run more than 4 tests per day"; randomize times; third-party clients "must maintain a published privacy policy that is consistent with" M-Lab's; commercial integrators "need to contribute to the platform"; "Offering compensation… to run a test is strictly prohibited."
- [Developing with M-Lab](https://www.measurementlab.net/develop/): "measurements must be actively initiated based on informed consent provided prior to installation of the client or initiation of the test", with a link to M-Lab's policies. "There is no API key needed." Platform rate limit 40 tests/client/day (204 beyond). Integrations ≤ 4 scheduled tests/day via Poisson process; best-effort SLO.
- **What we must tell users** (before the first M-Lab test, every time it's selected until acknowledged): this test goes to Measurement Lab; your IP address, test time and results are published as public open data and kept indefinitely; link to M-Lab privacy policy. Plus our own published privacy policy.

#### Rust clients

- [`ndt7-client`](https://crates.io/crates/ndt7-client) 0.3.1, Apache-2.0, created 2026-02, ~200 downloads: young. Protocol is small enough to implement with `tokio-tungstenite`.
- Official clients: Go `ndt7-client-go`, JS `ndt7-js` (both Apache-2.0, unverified for Go).

#### Data per test

10 s down + 10 s up, single stream: e.g. 100 Mbps ≈ 125 MB down; at 1 Gbps ≈ 1.25 GB down. Single stream can under-read fast or high-RTT links.

**Verdict:** open protocol and servers, but public IP publication needs explicit consent. Good as an opt-in second backend, not the default.

---

### 4. Ookla Speedtest CLI

- [speedtest.net/apps/cli](https://www.speedtest.net/apps/cli): closed-source binary v1.2.0, maintained by Ookla. Page pitches "Use Speedtest in your programs by wrapping it in the programming language of your choice." No open license.
- [EULA](https://www.speedtest.net/about/eula), "Grant of License": "a limited, non-exclusive and non-transferable license to use the Software through a command line interface for your **personal, non-commercial use on a single personal computer** owned or otherwise controlled by you."
- Restrictions: "(e) rent, lease, lend, sell, sublicense, assign, **distribute, publish, transfer, or otherwise make available the Software, or any features or functionality of the Software, to any third party for any reason**"; "(b) modify, translate, adapt, or otherwise create derivative works"; "(c) reverse engineer…".
- [Terms of Use](https://www.speedtest.net/about/terms): no "robot, spider… or other device to crawl, scrape… or index any portion of the Services"; content use limited to "personal, non-commercial use".
- [Privacy policy](https://www.speedtest.net/about/privacy): collects IP and city/zip, device info, Wi-Fi SSID/BSSID (apps); "Ookla collects consent to share your Speedtest results with internet providers, hardware manufacturers and industry regulators"; results are licensed to third parties (except paid Speedtest Custom).

**Verdict:** bundling = distribution (forbidden); calling a user-installed copy still "makes available… features or functionality… to any third party" and routes users into Ookla's data licensing. Skip. The raw Ookla server protocol is undocumented and covered by the same terms. Skip too.

---

### 5. macOS `networkQuality`

- Built-in since **macOS Monterey (12)**; iOS/iPadOS 15 in Settings ([Apple support 101942](https://support.apple.com/en-us/101942)). Present at `/usr/bin/networkQuality` on macOS 26.6 (observed).
- Flags (`networkQuality -h`, `man networkQuality`, macOS 26.6): `-c [file]` JSON; `-s` sequential (separate down/up, needed for per-direction RPM); `-d` no download / `-u` no upload (both imply `-s`); `-v` verbose; `-C <url|file://>` config; `-I <iface>`; `-M <sec>` max runtime; `-f h1|h2|h3|L4S|noL4S`; `-p` iCloud Private Relay; `-r host`; `-k` no TLS verify; `-b`/`-B` Bonjour servers; `-S port` run a local server.
- JSON fields: `dl_throughput`, `ul_throughput` (bps), `base_rtt` (idle ms), `responsiveness` (RPM), `dl_responsiveness`/`ul_responsiveness` (with `-s`), `dl_flows`, `ul_flows`, `il_h2_req_resp`, `il_tcp_handshake_443`, `il_tls_handshake`, `lud_foreign_*`/`lud_self_*` latency-under-load arrays, `interface_name`, `os_version`, `start_date`, `end_date`, `error_code`, `error_domain`. Man page: "This is not meant to be a comprehensive list of fields."
- Default server: config `https://mensura.cdn-apple.com/api/v1/gm/config` (secondary sources: [cyberhost.uk](https://cyberhost.uk/the-hidden-macos-speedtest-tool-networkquality/), [korya/netquality](https://pkg.go.dev/github.com/korya/netquality@v0.1.1)); observed it returns `small`/`large`/`slurp` URLs on `mensura.cdn-apple.com`. Default URL compiled into the private framework: not verified directly.
- Data: man page "will use data on your Internet service plan". No official figure. One reported run: ~278 MB down + ~70 MB up (cyberhost.uk; anecdotal). Parallel by default, so it saturates both directions at once.
- Shelling out: no Apple terms found that forbid an app running a bundled system CLI; it's a documented man-page tool. Output format is unversioned ("not meant to be a comprehensive list"), so parse tolerantly. What Apple logs: unverified (Apple CDN; standard Apple privacy policy applies). Mac-only.
- Server is open: [github.com/network-quality/server](https://github.com/network-quality/server).

#### IETF "Responsiveness under Working Conditions"

- [draft-ietf-ippm-responsiveness-09](https://datatracker.ietf.org/doc/draft-ietf-ippm-responsiveness/) (2026-07-06, IPPM WG, intended Proposed Standard). RPM = 60 / RTT(s).
- Responsiveness = (Foreign + Loaded) / 2, where Foreign = 60000 / mean(TM(tcp_f), TM(tls_f), TM(http_f)) on new connections, Loaded = 60000 / TM(http_l) on load-generating connections; TM = 95% single-sided trimmed mean.
- Parameters: MAD 4, ID 5 s, TMP 95%, SDT 5%, MPS 100 probes/s, PTC 5%, INP 1, INC 1, MNP 16 connections. Aim for ~10 s tests.
- Interpretation: Poor ≤ 300 RPM (≥ 200 ms), Fair ~1000 RPM (~60 ms), Good ~6000 RPM (~10 ms), Excellent > 6000. Confidence Low/Medium/High by stability. Recommends reporting goodput and idle latency with RPM.

---

### Recommendation

| Backend | Use? | Why |
| --- | --- | --- |
| Cloudflare `__down`/`__up` | **Yes, default** | Global anycast edge, MIT reference method (p90, median latency, loaded latency), open AIM scoring, colo in headers, no account. Implement in Rust (parallel streams + loaded probes, our own byte/time caps). Prefer `h3.speed.cloudflare.com` for large transfers (no Referer gate). Don't POST to `__results`. Not formally licensed for third-party clients: identify via User-Agent, keep it behind a trait. |
| M-Lab NDT7 | **Opt-in, later (v0.2+)** | Open protocol, neutral, used by Google. But publishes IP + results publicly forever: needs explicit consent dialog, our own privacy policy, ≤ 4 automated tests/day, randomized. Single stream under-reads fast links. |
| macOS `networkQuality` | **Optional Mac-only extra (later)** | Built in, free, gives RPM and latency under load against Apple's CDN. Shell out with `-c -s`, parse tolerantly. Not cross-platform, format unversioned, data use not published. |
| Netflix fast.com | **No** | Undocumented API, token scraped from JS, Netflix ToU bars automated access, may rotate or break. |
| Ookla Speedtest CLI | **No** | EULA: personal non-commercial only, no distribution or making features available to third parties; Ookla licenses results to ISPs. |

## Research: measurement method (input for docs/method.md)

Date: 2026-10-23. Scope: how to measure download, upload, idle/loaded latency, jitter and (maybe) loss against Cloudflare's `speed.cloudflare.com/__down` / `__up` from a native Rust app (reqwest/tokio), plus "what this speed is good for" thresholds and data use.

Legend: **[src]** = read in the cited source. **[obs]** = observed by me with curl on 2026-10-23 (no IP/SSID recorded). **[derived]** = my arithmetic. **[unverified]** = secondary source or not confirmed.

---

### 1. How the reference tests work

#### 1.1 Cloudflare `@cloudflare/speedtest` (v1.15.0, MIT)

Sources: [README](https://github.com/cloudflare/speedtest), [defaultConfig.ts](https://github.com/cloudflare/speedtest/blob/main/src/config/defaultConfig.ts), [BandwidthEngine.ts](https://github.com/cloudflare/speedtest/blob/main/src/engines/BandwidthEngine/BandwidthEngine.ts), [MeasurementCalculations.ts](https://github.com/cloudflare/speedtest/blob/main/src/Results/MeasurementCalculations.ts), [internalConfig.ts](https://github.com/cloudflare/speedtest/blob/main/src/config/internalConfig.ts), [blog: how it works](https://blog.cloudflare.com/how-does-cloudflares-speed-test-really-work/).

- **Sequential, one request at a time** ("Measures download and upload bandwidth via sequential HTTP requests" — BandwidthEngine.ts). No parallel streams. [src]
- **Fixed size ramp** (source v1.15.0; README lists an older variant without interleaved 2-packet latency steps) [src]:
  - latency ×2 → down 100 kB ×1 → latency ×20 → down 100 kB ×9 → down 1 MB ×8 → up 100 kB ×8 → up 1 MB ×6 → down 10 MB ×6 → up 10 MB ×4 → down 25 MB ×4 → up 25 MB ×4 → down 100 MB ×3 → up 50 MB ×3 → down 250 MB ×2 (2 latency probes between each set).
  - **Stop rule:** once any request in a direction takes ≥ `bandwidthFinishRequestDuration` = 1000 ms, larger sets in that direction are skipped.
- **Per-request throughput:** download duration = TTFB − server time + payload time; upload duration = TTFB of the response (server replies after receiving the body). bits = 8 × transferSize (or bytes × 1.005 header estimate). [src]
- **Server time subtraction:** parses `server-timing` header — `cfReqDur;dur=` if present, else sum of all `cfSpeed*;dur=` entries; also calibrates a `serverTimeDelta` from new HTTP/1.1 connections. [src] Observed header: `Server-Timing: cfSpeedEdge;dur=3, cfSpeedWorker;dur=17..65` — i.e. 20–70 ms of server time that must be subtracted. [obs]
- **Final download/upload:** **90th percentile** of per-request bps, only requests ≥ 10 ms (`bandwidthMinRequestDuration`). [src] Note: p90 of per-request speeds leans toward the fast end — it is not a sustained mean.
- **Latency:** median (`latencyPercentile` 0.5) of zero-byte request RTTs (TTFB − server time). [src]
- **Jitter:** mean absolute difference of consecutive RTTs: Σ|RTTᵢ − RTTᵢ₋₁| / (n−1). [src]
- **Loaded latency:** side-channel zero-byte requests every 400 ms (`loadedLatencyThrottle`) during transfers; only counted for sets whose shortest request ≥ 250 ms (`loadedRequestMinDuration`, i.e. really loading); keep last 20 points; median. Reported per direction. [src]
- **Packet loss:** opt-in only, via WebRTC **TURN** (100 UDP packets, batches of 10, wait 5 s); needs a TURN server + credentials; not in the default sequence. [src]
- **Data used (derived from the sequence):** all sets = ~969 MB down + ~297 MB up. Early stop limits it: at 100 Mbps a 25 MB request takes 2 s → stops after that set ≈ 169 MB down; at 1 Gbps every set runs ≈ 969 MB down; at 10 Mbps the 10 MB set alone is 6 × 8 s ≈ 48 s. [derived]

Endpoint facts [obs, 2026-10-23, undocumented — re-check before relying on them]:
- Served over **HTTP/1.1** even when curl offered h2 (ALPN). So "N parallel streams" = N TCP connections; good for us.
- `__down?bytes=N`: N ≤ 99,999,999 → 200; **N ≥ 100,000,000 → 403 unless `Referer`/`Origin` is speed.cloudflare.com**. We must not spoof those headers → keep requests < 100 MB (we only need ≤ 25 MB).
- `__down` response headers include `colo`, `city`, `country`, `asn`, `latitude`, `longitude`, `cf-meta-ip` (privacy: never log/store anything but `colo`). `__up` returns `cf-meta-colo` and `cf-meta-upload-bytes` (server-side byte count, useful to verify upload accounting).
- `https://speed.cloudflare.com/cdn-cgi/trace` returns `colo=` (IATA code) — usable for "Cloudflare, Frankfurt".
- `https://speed.cloudflare.com/meta` returned `{}` from curl. [obs]
- Upload size limit for `__up`: **[unverified]** — Cloudflare's own client goes up to 50 MB.

#### 1.2 Netflix fast.com

Sources: fast.com app JS (`https://fast.com/app-0bffe1.js`, read 2026-10-23) [src]; [fast.com page](https://fast.com/) text. The Netflix tech blog post ("Building fast.com") returned 403 to my fetcher.

- **Parallel connections:** UI setting defaults min 1, max 8 (`minConnections:{default:1}`, `maxConnections:{default:8}`); tester library default `connections:{min:1,max:3}`, `maxConnections:8`. Adaptively adds connections. [src]
- **Request sizes:** HTTP range requests on Netflix OCA URLs; first request 2 KB (`firstRequestBytes:2048`), sized so a range request takes 300 ms growing by 200 ms per step up to 1000 ms (`startRangeRequestTimeMs:300`, `rangeRequestIncreaseMs:200`, `maxRangeRequestTimeMs:1e3`); max payload 25 MiB (`MAX_PAYLOAD_BYTES=26214400`); `maxBytesInFlight` 75 MiB. 5 server URLs (`urlCount:5`). [src]
- **Sampling:** progress every 150 ms on the page (`progressFrequencyMs:150`; library default 200). [src]
- **Warm-up discard (aggregator `stableMovingAverage(5)`):** computes a 5-snapshot moving average while it keeps rising; the first time the window average falls below the best so far, it fixes the start index there and from then on reports cumulative bytes/time from that point. I.e. ramp-up is discarded automatically, then a long-window mean. [src]
- **Stop rule (`stableDeltaMeasurementsStopper`):** minDuration 7 s, maxDuration 30 s, stabilityDelta 2 %, minStableMeasurements 6: stop when the last 6 progress values are all within 2 % of the current speed and the peak is not in the most recent half-window, and ≥ 7 s elapsed. Settings UI: duration min 5 / max 30 s. [src]
- **Latency:** unloaded = min over servers of the **10th percentile** of zero-range (`/range/0-0`) request times; loaded = min over servers of the **75th percentile** (needs > 4 samples). Page explains the difference "is also called 'bufferbloat'". [src]
- fast.com's page says it "calculates the maximum speed your Internet connection can provide". [src]

#### 1.3 Ookla Speedtest

Ookla's methodology pages (ookla.com, speedtest.net) refused my fetcher. Secondary sources:
- [MacMillan et al., "A Comparative Analysis of Ookla Speedtest and M-Lab NDT7" (arXiv 2205.12376)](https://arxiv.org/abs/2205.12376): legacy HTTP client "sampled throughput 20 times and discarded the top 2 and the bottom 25% samples, thus, effectively disregarding the TCP slow start period"; current client's sampling "is not public" but still "discards low throughput samples". Ookla adapts connection count (multiple connections from ~20 ms RTT, up to 8 at > 100 ms RTT) and duration (median 3.5 s at 0 ms RTT → 11 s at 100 ms → 15.7 s at 600 ms). [src, paper]
- [Feamster & Livingood, "Internet Speed Measurement: Current Challenges and Future Recommendations" (arXiv 1905.02334)](https://arxiv.org/abs/1905.02334): "Past research concluded that a speed test should have at least four parallel connections"; browsers open "as many as six parallel connections to a single server"; tests should exclude slow start ("Many standard throughput tests, including the FCC/SamKnows test, omit the initial slow start period"); methods differ ("may discard some high and/or low results, may use the median or the mean, may take only the highest result"); calls for a "well documented, public methodology". [src]

#### 1.4 M-Lab NDT7

Source: [ndt7 protocol spec](https://github.com/m-lab/ndt-server/blob/main/spec/ndt7-protocol.md) (v0.11.0).
- **Single TCP connection** over WebSocket/TLS; test ≤ 10 s ("maximum runtime ... ten seconds"); uses TCP BBR server-side where available. [src]
- Messages 1 KiB–16 MiB, start at 8 KiB, doubling when smaller than 1/16 of bytes queued so far (non-normative). [src]
- Main metric: application-level goodput = `AppInfo.NumBytes / AppInfo.ElapsedTime` — i.e. **mean over the whole test, including slow start**. [src] Feamster & Livingood criticise this (single stream + slow start included → under-reads above ~100 Mbps). [src]
- Measurement messages ≤ 10/s on average. [src]
- (Open-data publication of results incl. IP: not in the spec — covered by the backend research, not here.)

#### 1.5 Apple `networkQuality` / IETF Responsiveness

Sources: `man networkQuality` (macOS, read locally) [src]; [draft-ietf-ippm-responsiveness-09](https://datatracker.ietf.org/doc/draft-ietf-ippm-responsiveness/) (updated 2026-07-06) [src].
- macOS tool measures capacity and responsiveness in **RPM**; parallel up+down by default, `-s` sequential, `-M` max runtime, `-c` JSON (`base_rtt` = idle latency). [src]
- Draft method: start with 1 load-generating connection, add 1 per 5 s interval, recommended max 16; "moving average aggregate goodput" over MAD = 4 intervals; **saturation when stdev of last 4 averages < 5 % (SDT)** of current; then same test on responsiveness. "Ideally within ten seconds". Confidence Low/Medium/High by whether stability was reached. [src]
- Probes: **foreign** (separate connections: TCP, TLS, HTTP times) and **self** (on the loaded connections). Aggregate: single-sided 95 % trimmed mean. RPM = (60000/mean(foreign) + 60000/TM(http_l)) / 2. RPM = 60 / RTT[s]. [src]
- Categories (draft §1.2): **Poor ≤ 300 RPM (≥ 200 ms)**, **Fair 300–1000 RPM (60–200 ms)**, **Good 1000–6000 RPM (10–60 ms)**, **Excellent ≥ 6000 RPM (≤ 10 ms)**. [src]

#### 1.6 Waveform bufferbloat test

waveform.com is behind a Cloudflare challenge; could not read it. Secondary only **[unverified]**: grade by average added latency under load (down+up): A+ < 5 ms, A < 30 ms, B < 60 ms, C < 200 ms, D < 400 ms, F ≥ 400 ms ([OpenWrt forum post](https://forum.openwrt.org/t/2025-current-recommendations-for-bufferbloat-testing/241303/1); A+ < 5 ms also in [speedtesthq guide](https://speedtesthq.com/guides/diagnostics/test-for-bufferbloat)). Use as inspiration, not as a cited standard.

---

### 2. Key design questions, with evidence

#### 2.1 Parallel streams
- Single stream (NDT7, Cloudflare sequential) under-reads on fast or long-RTT links (slow start, loss sensitivity) — Feamster & Livingood; MacMillan et al. show Ookla's multi-connection reads higher at high RTT. [src]
- ≥ 4 connections recommended (Feamster & Livingood), browsers use ≤ 6 per host, fast.com up to 8, Ookla up to 8, IETF up to 16 but adds slowly (1 per 5 s — too slow for a 20 s test). [src]
- Cloudflare edge speaks HTTP/1.1 here [obs]; even so, force HTTP/1.1 in reqwest (`http1_only`) with one client per stream (or `pool_max_idle_per_host` and explicit distinct connections) so streams are real separate TCP connections and never multiplexed by HTTP/2.

#### 2.2 Warm-up and the statistic
- Slow start must be excluded or the number under-reads (Feamster & Livingood; FCC/SamKnows omit it). [src]
- Options: discard low samples (Ookla — biases upward), p90 of per-request rates (Cloudflare — upward-leaning), moving average from the point the ramp stops (fast.com — sustained mean), whole-test mean (NDT7 — downward bias). Honest middle: **fast.com-style ramp detection + mean of the steady window** (total bytes ÷ total time after warm-up). It is what the link sustains, not a peak.
- Byte counters vs per-request timing: in a native app the download byte counter is exact (bytes read from sockets) and gives continuous samples. For **upload**, bytes written to the socket ≠ bytes delivered (kernel send buffer can hold MBs), so the counter over-reads near the start/end; per-request completion (server responds after receiving the body; Cloudflare uses upload TTFB) is the ground truth, and `cf-meta-upload-bytes` confirms the count. [src/obs]

#### 2.3 Latency
- Cloudflare: HTTP zero-byte requests, TTFB minus `server-timing`; median of 20. fast.com: 10th percentile of zero-range requests (min-leaning). IETF: TCP connect, TLS handshake, HTTP request times averaged. [src]
- ICMP is not needed for the speed test: TCP connect time to the edge is a clean kernel-level RTT (no server processing) and needs no privileges; HTTP RTT on a warm connection matches what Cloudflare reports. Server time is 20–70 ms on this endpoint [obs], so subtraction is mandatory for HTTP probes.
- **Jitter:** Cloudflare = mean |Δ| of consecutive RTTs [src]. RFC 3550 §6.4.1 interarrival jitter is a running estimator `J(i) = J(i-1) + (|D(i-1,i)| - J(i-1))/16` designed for continuous RTP streams ([RFC 3550](https://www.rfc-editor.org/rfc/rfc3550#section-6.4.1)) [src] — with ~20 probes its 1/16 gain never converges, so mean |Δ| is the right choice and keeps numbers comparable with Cloudflare. Stdev is not used by any reference.
- **Loaded latency:** probe on a **separate** connection while loaded (IETF "foreign" probe; Cloudflare side-channel); only count probes taken while the link is actually saturated (Cloudflare's ≥ 250 ms rule; fast.com uses p75). [src]

#### 2.4 Packet loss
- Cloudflare's loss test needs a TURN server and credentials (opt-in; example Worker provided). [src] No UDP echo service on `speed.cloudflare.com`.
- Without root: macOS unprivileged ICMP (`SOCK_DGRAM`) and Windows `IcmpSendEcho` exist, but routers and anycast edges rate-limit/deprioritise ICMP, so ICMP loss misreads real loss. **[unverified — needs the diagnose-research agent]**
- Recommendation: **do not report packet loss in v0.1.** Report failed/timed-out probes honestly as "N of 20 checks didn't answer" only when > 0. Revisit with a TURN server we'd have to run (costs; conflicts with "free only") — likely never.

#### 2.5 Data use (per direction, at full rate) [derived]
MB per second = Mbps / 8.

| Link | 1 s | 5 s | 8 s | 10 s |
|---|---|---|---|---|
| 10 Mbps | 1.25 MB | 6 MB | 10 MB | 12.5 MB |
| 100 Mbps | 12.5 MB | 63 MB | 100 MB | 125 MB |
| 500 Mbps | 62.5 MB | 313 MB | 500 MB | 625 MB |
| 1000 Mbps | 125 MB | 625 MB | 1000 MB | 1250 MB |

Duration-only tests cost ~1 GB+ per test at gigabit (fast.com up to 30 s; Cloudflare ~970 MB down). A byte cap is needed. Wire bytes are ~3–5 % above payload (TCP/IP/TLS headers; Cloudflare's own estimate is +0.5 % for HTTP headers) — round estimates up.

---

### 3. "What this speed is good for" — authoritative thresholds

| Use | Threshold | Source |
|---|---|---|
| Netflix HD 720p | ≥ 3 Mbps | [Netflix help](https://help.netflix.com/en/node/306) [src] |
| Netflix Full HD 1080p | ≥ 5 Mbps | same [src] |
| Netflix 4K | ≥ 15 Mbps (per stream) | same [src] |
| Zoom 1:1 720p | 1.2 Mbps up/down | [Zoom KB0060748](https://support.zoom.com/hc/en/article?id=zm_kb&sysparm_article=KB0060748) [src] |
| Zoom 1080p (1:1 or group) | 3.8 up / 3.0 down Mbps | same [src] |
| Zoom group 720p | 2.6 up / 1.8 down Mbps | same [src] |
| Teams video, recommended | 1:1 1.5/1.5; meetings 2.5 up / 4.0 down Mbps; best 4/4 | [Microsoft Learn: Prepare network](https://learn.microsoft.com/en-us/microsoftteams/prepare-network) [src] |
| Teams "good" network | jitter < 30 ms, loss < 5 %, RTT < 500 ms (lenient, admin telemetry) | [Microsoft Learn: real-time telemetry](https://learn.microsoft.com/en-us/microsoftteams/use-real-time-telemetry-to-troubleshoot-poor-meeting-quality) [src] |
| Teams Wi-Fi signal | Excellent ≥ −50 dBm, Good −51…−64, Poor ≤ −65 dBm | same [src] (useful for the Wi-Fi diagnosis) |
| US broadband benchmark | 100/20 Mbps (2024 Report FCC 24-27; **kept** in the 2026 Section 706 Report FCC 26-55, adopted 2026-08-13, which **abolished** the 1000/500 long-term goal) | [FCC 24-27](https://docs.fcc.gov/public/attachments/FCC-24-27A1.pdf), [FCC 26-55 ¶16–17](https://docs.fcc.gov/public/attachments/FCC-26-55A1.pdf) [src] |
| US BEAD latency standard | 95 % of RTT measurements ≤ 100 ms | [NTIA Performance Measures Policy Notice](https://broadbandusa.ntia.gov/sites/default/files/2025-09/Performance_Measures_Policy_Notice.pdf) (via search summary; PDF not read directly) [unverified detail] |
| Responsiveness | Poor ≥ 200 ms, Fair 60–200, Good 10–60, Excellent ≤ 10 ms RTT under load | [IETF draft-09 §1.2](https://datatracker.ietf.org/doc/draft-ietf-ippm-responsiveness/) [src] |

**Cloudflare AIM scoring** ([internalConfig.ts](https://github.com/cloudflare/speedtest/blob/main/src/config/internalConfig.ts); [docs](https://developers.cloudflare.com/speed/aim/)) [src]. Points per metric (thresholds are upper bounds, value < threshold gets the points to its left):
- latency (ms): <10 → 20, <20 → 10, <50 → 5, <100 → 0, <500 → −10, else −20
- loadedLatencyIncrease = max(down-loaded, up-loaded) − idle (ms): same scale as latency
- jitter (ms): <10 → 10, <20 → 5, <100 → 0, <500 → −10, else −20
- download / upload: <1 Mbps → 0, <10 → 5, <50 → 10, <100 → 20, ≥100 → 30
- packetLoss (ratio): <1 % → 10, <5 % → 5, <25 % → 0, <50 % → −10, else −20 (missing = 0 points unless omitted)
- Experiences (sum ≥ threshold → bad/poor/average/good/great):
  - streaming = latency + packetLoss + download + loadedLatencyIncrease; thresholds 15/20/40/60
  - gaming = latency + packetLoss + loadedLatencyIncrease; 5/15/25/30
  - rtc (video chat) = latency + jitter + packetLoss + loadedLatencyIncrease; 5/15/25/40
- Caveat: without packet loss our scores can't equal Cloudflare's (they'd be ≤ 10 points lower). Don't show "AIM"; borrow the boundaries.

**Proposed plain-language guidance (our rules, derived from the above):**
- 4K video: download ≥ 25 Mbps → "Good for 4K video" (Netflix 15 Mbps per stream + headroom). 15–25 → "4K on one screen". ≥ 5 → "HD video". ≥ 3 → "720p video". Below → "Video may buffer".
- Video calls: up ≥ 4 and down ≥ 4 Mbps (Teams best / Zoom 1080p) and loaded latency < 100 ms and jitter < 30 ms → "Good for video calls". Loaded latency 100–200 ms → "Calls may stutter: latency jumps to N ms under load". ≥ 200 ms → "Calls will lag when the line is busy".
- Gaming: idle < 50 ms and increase under load < 50 ms → "Fine for online games"; < 20/< 20 → "Good for competitive games" (AIM boundaries 20/50/100).
- Household: ≥ 100/20 → "Meets the FCC broadband benchmark (100/20)". (US-specific — show only as a footnote/report line.)
- Bufferbloat label from increase under load: < 30 ms "Steady under load", 30–100 ms "Slows under load", ≥ 100 ms "Lags under load" (Waveform-like bands [unverified] + AIM 100 ms boundary).

---

### 4. Proposed method (for docs/method.md)

Target: whole test ≈ 12–20 s, ≤ 21 s worst case. One server: Cloudflare anycast (`colo` from `cdn-cgi/trace` or `cf-meta-colo`).

**0. Pre-flight (≤ 1 s, ~10 kB).** Resolve DNS, `GET /cdn-cgi/trace` → colo. Abort with "No internet" if it fails within 5 s.

**1. Idle latency (~2 s, < 50 kB).**
- Open 1 probe connection (HTTP/1.1, keep-alive). Record TCP connect times of that and the stream connections opened in parallel now (pre-warm; also free extra RTT samples).
- 20 probes `GET __down?bytes=0`, one at a time, ≥ 50 ms apart. RTT = TTFB − server time (sum of `cfSpeed*;dur=` in `Server-Timing`, or `cfReqDur` if present); if result ≤ 1 ms or header missing, use TTFB. Discard the first probe (connection warm-up).
- Latency = **median**; jitter = **mean |Δ| of consecutive RTTs** (Cloudflare formula). Show TCP-connect median in details as a cross-check.
- Valid if ≥ 10 of 20 probes succeed (timeout 2 s each).

**2. Download (min 5 s, max 10 s, cap 400 MB).**
- **Streams:** 6 parallel HTTP/1.1 connections (8 if idle latency > 100 ms, e.g. satellite/mobile). Each stream loops requests.
- **Request size ramp per stream:** first 256 kB, then size = clamp(stream_rate × 1 s, 256 kB, 25 MB) — each request ≈ 1 s so the per-request RTT gap is ≤ ~5 %, and stays under the 100 MB Referer gate. Add `measId`/random query to avoid caches.
- **Sampling:** atomic byte counter across streams, ticked every 100 ms (monotonic `Instant`); aggregated into 250 ms samples for stats; UI gets a 1 s moving average at 10 Hz (smooth count-up).
- **Warm-up discard:** steady window starts at the first 250 ms sample where the 1 s moving average stops rising (< 3 % above its previous max for 2 consecutive samples), but not earlier than 1.0 s and not later than 4.0 s after start (at 4 s, start anyway and mark "ramped slowly").
- **Stop:** when steady window ≥ 3 s **and** total ≥ 5 s **and** the last 6 steady samples' running mean moved < 3 % (fast.com uses 6 samples within 2 %); or at 10 s; or at 400 MB transferred. On stop, cancel all streams (drop connections); count only bytes received before the stop instant.
- **Result = steady-window mean** = bytes in steady window × 8 ÷ steady duration. Also store p10/p50/p90 of 250 ms steady samples to show "varied 280–330 Mbps" when (p90 − p10)/mean > 25 %. Never show the max.

**3. Loaded latency (during 2 and 4, ~1 kB per probe).**
- Dedicated probe connection opened in step 1 (separate TCP = IETF "foreign" probe).
- One probe every 250 ms; count only probes **sent inside the steady window**; median per direction; jitter under load = mean |Δ|.
- A probe that times out (2 s) counts as 2000 ms (censored), not dropped — dropping would hide bufferbloat. Need ≥ 5 counted probes per direction, else "Not measured".
- Show "Latency 14 ms · 62 ms under load" where under-load = max(download-loaded, upload-loaded) (AIM's definition of the increase uses the max too).

**4. Upload (min 5 s, max 8 s, cap 200 MB).**
- 4 streams (6 if idle latency > 100 ms). Pause 300 ms after download so queues drain.
- `POST __up` with body of size clamp(stream_rate × 1 s, 256 kB, 25 MB) (stay ≤ Cloudflare's own 50 MB; real limit **unverified** — confirm with wiremock-free manual test once). Reuse one pre-generated zero/random buffer (`Bytes` clone, no per-request alloc).
- **Live gauge:** bytes handed to the socket (counter), 1 s moving average.
- **Result:** per stream, from requests that *started* inside the steady window and completed: Σbytes ÷ Σ(response TTFB − server time); total = Σ stream rates. Cross-check against `cf-meta-upload-bytes`. Same warm-up and stop rules as download (warm-up detected on completed-request rates; fall back to counter-based if < 3 completed requests per stream, flagged "rough").

**5. Validity rules — a phase shows no number unless all hold.**
- Steady window ≥ 2 s (≥ 1 s only when stopped by the byte cap; then label "Stopped at data limit — speed may be higher").
- No total stall: zero bytes on all streams for 3 s → fail "Connection stalled".
- ≥ 50 % of streams alive through the steady window; one stream may be restarted once.
- HTTP status 200 only; 403/429/5xx → fail "Cloudflare refused the test" / "server error" (never retry silently more than once).
- Network path change (interface/SSID/gateway change, sleep/wake) during the test → abort "Network changed during the test".
- User cancel → nothing saved as a result (history may log "Cancelled").
- Latency invalid (< 10/20 probes) → speed can still be valid; latency shows "Not measured".
- Failed download → don't run upload (save data); report what failed, no partial numbers.

**6. Data cap and estimate.**
- Caps: 400 MB down + 200 MB up + < 1 MB probes → hard ceiling ~600 MB. [Owner decision: offer "Data per test: Standard (up to 600 MB) / Light (up to 100 MB: 75 down, 25 up — accurate to ~200 Mbps)".]
- **Estimate before a test:** E = min(cap_d, R_d/8 × T_d) + min(cap_u, R_u/8 × T_u) + 1 MB, × 1.05 wire overhead, with R = last result on this network (median of last 3), T_d = 7 s, T_u = 6 s (typical stop times; tune from history). No history → show the ceiling: "Uses up to 600 MB". Round: < 50 MB to 5 MB, else to 50 MB.
  - [derived] 100/20 Mbps → 88 + 15 + 1 → ~110 MB. 300/40 → 263 + 30 → ~300 MB. 1000/100 → 400 + 75 → ~500 MB.
- After the test, show actual bytes counted ("Used 212 MB").

**7. Expected comparison with speed.cloudflare.com (for M1 verify).**
- Our 6-stream steady mean vs Cloudflare's single-stream p90: on ≤ 300 Mbps low-RTT links expect within ~10 %; on gigabit or high-RTT links we may read higher (parallel streams) — expected, documented. [derived, to verify on owner's Mac; record in docs/verification.md without IP]

### 5. Open items
- Confirm `__up` max body size, and whether Cloudflare rate-limits repeated native tests (no ToS read here).
- Confirm HTTP/1.1-only on owner's network (my env may sit behind a proxy).
- ICMP/UDP loss feasibility belongs to the diagnose research.
- Read Ookla's official methodology page in a browser (blocked for my fetcher).

## Platform research: metered, Wi-Fi, gateway, ICMP, DNS, probes, battery, tray

Date: 2026-10-23. Targets: macOS 26 Tahoe, Windows 11 24H2/25H2. App: Tauri 2, unsigned/ad-hoc, not sandboxed, no admin.

Legend: **[verified]** = checked in an official doc, crate source, or run locally on the dev Mac (macOS 26.6.2, uid 501, no Location grant). **[unverified]** = community source or inference; test before relying on it.

Crate versions on crates.io today: `windows` 0.62.2, `windows-sys` 0.61.2, `objc2-core-wlan` 0.3.2, `objc2-core-location` 0.3.2, `block2` 0.6.2, `dispatch2` 0.3.1, `netdev` 0.46.3, `surge-ping` 0.9.1, `hickory-resolver` 0.26.3, `socket2` 0.6.5, `window-vibrancy` 0.8.1, `tauri-plugin-positioner` 2.4.0, `networkframework` 0.14.0, `objc2-network` 0.0.0 (placeholder).

---

### 1. Metered / expensive link detection

#### macOS
- `nw_path_is_expensive(path)`: "Checks whether the path uses an interface that is considered expensive, such as Cellular or a Personal Hotspot." ([Apple nw_path_t](https://developer.apple.com/documentation/network/nw_path_t)). `nw_path_is_constrained` = Low Data Mode ([NWPath.isConstrained](https://developer.apple.com/documentation/network/nwpath/isconstrained)).
- Low Data Mode on Mac is per Wi-Fi network (System Settings → Wi-Fi → network Details) ([AppleMagazine](https://applemagazine.com/low-data-mode/)) **[unverified exact path]**.
- iPhone hotspot: Mac joined to an iPhone hotspot reports `isExpensive = true`; `isConstrained` stays false even if the phone has Low Data Mode ([MacRumors thread, Big Sur era](https://forums.macrumors.com/threads/so-no-low-data-mode-in-big-sur-what-do.2253553/)) **[unverified on 26]**.
- Android hotspots signal metered via DHCP option 43 `ANDROID_METERED`; iOS ignores it, no source says macOS reads it ([OpenWrt](https://openwrt.org/docs/guide-user/network/wifi/android-meteredconnection), [GL.iNet forum](https://forum.gl-inet.com/t/how-to-detect-metered-connection/3827)). So an Android hotspot may look *unmetered* on a Mac **[unverified]**. Mitigation: also treat "Wi-Fi gateway is a phone" heuristics as unknown and keep full tests manual by default anyway.
- **Local check [verified]:** a Swift `NWPathMonitor` on this Mac printed `status satisfied, expensive true, constrained false, wifi true`. So the dev Mac is on an expensive link right now (likely a hotspot). Do not run real speed tests from this machine until that changes.
- **Rust bindings:**
  - `objc2-network` is an empty 0.0.0 placeholder ([docs.rs](https://docs.rs/objc2-network/latest/objc2_network/)). Don't use.
  - `networkframework` 0.14 ([docs.rs](https://docs.rs/networkframework)): MIT/Apache, `PathMonitor`, but backed by a Swift bridge, macOS 14+ target; would add a Swift build step. Not worth it for 3 functions.
  - **Recommended:** hand-written `extern "C"` against `Network.framework` (`#[link(name = "Network", kind = "framework")]`): `nw_path_monitor_create`, `nw_path_monitor_set_queue`, `nw_path_monitor_set_update_handler`, `nw_path_monitor_start`/`cancel`, `nw_path_get_status`, `nw_path_is_expensive`, `nw_path_is_constrained`, `nw_path_uses_interface_type`. Update handler is a block → `block2::RcBlock`; queue via `dispatch2`. ~80 lines, no permission, no prompt.
- Permissions: none.

#### Windows
- WinRT `NetworkInformation::GetInternetConnectionProfile()` → `ConnectionProfile::GetConnectionCost()` → `ConnectionCost` with `NetworkCostType` (Unrestricted/Fixed/Variable/Unknown), `Roaming`, `OverDataLimit`, `ApproachingDataLimit`, `BackgroundDataUsageRestricted` ([MS Learn ConnectionCost](https://learn.microsoft.com/en-us/uwp/api/windows.networking.connectivity.connectioncost)). MS guidance: "Fixed: gate large optional downloads (offer user confirmation)"; "Variable: minimize non-essential transfers". `GetInternetConnectionProfile` returns null when no suitable profile ([MS Learn](https://learn.microsoft.com/en-us/uwp/api/windows.networking.connectivity.networkinformation.getinternetconnectionprofile)). Subscribe to `NetworkInformation.NetworkStatusChanged` and re-read.
- `windows` crate feature: `Networking_Connectivity` (+ `Foundation`) **[verified on docs.rs 0.62.2 feature list]**. These WinRT classes are callable from unpackaged Win32 apps (no capability needed for cost) **[unverified by doc text; widely used; test in VM]**.
- Win32 alternative, documented "desktop apps only": COM `INetworkCostManager::GetCost` → `NLM_CONNECTION_COST_*` flags ([MS Learn](https://learn.microsoft.com/en-us/windows/win32/api/netlistmgr/nn-netlistmgr-inetworkcostmanager)); feature `Win32_Networking_NetworkListManager`. Use as fallback if WinRT misbehaves.
- Windows marks phone hotspots metered via AP beacon vendor elements, not DHCP ([GL.iNet forum](https://forum.gl-inet.com/t/how-to-detect-metered-connection/3827)) **[unverified]**.
- Permissions: none.

**Rule:** treat `expensive || constrained` (mac) and `Fixed|Variable || Roaming || OverDataLimit || ApproachingDataLimit || BackgroundDataUsageRestricted` (win) as "needs confirmation"; "Unknown" cost → treat as metered for automatic tests.

---

### 2. Wi-Fi signal and rate

#### macOS (CoreWLAN)
- **Local check [verified, macOS 26.6.2, no Location grant, swift CLI]:** `CWWiFiClient.shared().interface()` returned `rssiValue -42`, `noiseMeasurement -84`, `transmitRate 229.0`, `wlanChannel` non-nil, `activePHYMode` 6 (802.11ax), `powerOn true`; **`ssid` nil, `bssid` nil**. So signal, noise, SNR, rate, channel, band and PHY work without Location; only SSID/BSSID are gated.
- Apple: "SSID information is not available unless Location Services is enabled and the user has authorized the calling app to use location services" (CWInterface.h, quoted by DTS in [forum 732431](https://developer.apple.com/forums/thread/732431?page=2)). Started in macOS 14 for the current network. Daemons can't get the Location privilege; DTS advises a real app bundle signed with a stable identity.
- Getting SSID would need: `NSLocationUsageDescription` (and `NSLocationWhenInUseUsageDescription`) in Info.plist, `CLLocationManager.requestWhenInUseAuthorization` (crate `objc2-core-location`). Ad-hoc signed apps: TCC pins ad-hoc grants to the cdhash, so each update can silently drop the grant ([OpenClaw mac permissions doc](https://docs.openclaw.ai/platforms/mac/permissions.md), about Accessibility; same mechanism likely for Location) **[unverified for Location]**. `tccutil reset` doesn't support CoreLocation ([forum 757858](https://developer.apple.com/forums/thread/757858)). Results were inconsistent across processes even with a grant ([forum 748518](https://developer.apple.com/forums/thread/748518)).
- Rust: `objc2-core-wlan` 0.3.2, feature `CWInterface` (+ `CWChannel`, `CoreWLANTypes`); methods `rssiValue`, `noiseMeasurement`, `transmitRate`, `wlanChannel`, `activePHYMode` ([docs.rs](https://docs.rs/objc2-core-wlan/latest/objc2_core_wlan/struct.CWInterface.html)). Wrap every call in `objc2::rc::autoreleasepool`: netdev found CoreWLAN's XPC objects leak "multi-MB/hour" on long-lived threads otherwise ([netdev src/os/macos/wifi.rs](https://github.com/shellrow/netdev/blob/main/src/os/macos/wifi.rs)) **[verified in source]**.

#### Windows
- **24H2 change [verified, MS doc]:** without precise-location consent, `WlanQueryInterface(wlan_intf_opcode_current_connection)`, `WlanGetNetworkBssList`, `WlanGetAvailableNetworkList`, `WlanScan` return `ERROR_ACCESS_DENIED`; first call shows a one-time-per-app system prompt; calls show the "location in use" tray icon ([MS Learn: Wi-Fi access and location changes](https://learn.microsoft.com/en-us/windows/win32/nativewifi/wi-fi-access-location-changes)). The whole `current_connection` call fails, so signal quality and rate from it are lost too. Catchpoint confirms "BSSID, SSID, and signal quality will no longer be available" without Location ([Catchpoint](https://docs.catchpoint.com/docs/endpoint-wifi-reporting-on-win11-24h2)). `netsh wlan show interfaces` is also blocked ([Elevenforum](https://www.elevenforum.com/t/netsh-wlan-query-access-is-denied.45302/post-719708)).
- Not in Microsoft's affected list, so likely usable without Location **[unverified, test in VM]**:
  - WinRT `ConnectionProfile.GetSignalBars()` → 0–5 bars, `IsWlanConnectionProfile` ([MS Learn](https://learn.microsoft.com/en-us/uwp/api/windows.networking.connectivity.connectionprofile.getsignalbars)). Coarse but enough for "Wi-Fi weak".
  - `WlanQueryInterface(wlan_intf_opcode_rssi)` → LONG dBm ([opcode enum](https://learn.microsoft.com/en-us/windows/win32/api/wlanapi/ne-wlanapi-wlan_intf_opcode)); only `current_connection` is named as gated.
  - Link rate via `GetIfEntry2` / `GetAdaptersAddresses` `TransmitLinkSpeed`/`ReceiveLinkSpeed` (netdev exposes `transmit_speed`/`receive_speed` on Windows) **[verified in netdev source]**.
- Feature names: `Win32_NetworkManagement_WiFi`, `Networking_Connectivity` **[verified]**.

**Recommendation:** never read SSID/BSSID. No Location prompt on either OS. Use RSSI+noise+rate (mac) and signal bars/RSSI opcode + link speed (win). For per-network history later, key networks by a local salted hash of the gateway MAC (ARP/neighbor table, not location-gated per MS list; netdev reads it) and let the user name them. Fits the "never store SSID" rule.

---

### 3. Default gateway without root

- **`netdev` 0.46.3** (MIT; renamed from `default-net`) ([GitHub](https://github.com/shellrow/netdev), [docs.rs](https://docs.rs/netdev/latest/netdev/)): `get_default_interface()`, `get_default_gateway()`; `Interface` has `gateway: Option<NetworkDevice>` (IP + MAC), `dns_servers: Vec<IpAddr>`, `transmit_speed`, `receive_speed`, `if_type`, `oper_state`, `stats` **[verified in source]**.
  - macOS: routing table via `sysctl(CTL_NET, PF_ROUTE, …, NET_RT_FLAGS/NET_RT_DUMP)`; neighbors (gateway MAC) via sysctl too. No root.
  - Windows: `GetAdaptersAddresses` (FirstGatewayAddress, FirstDnsServerAddress) + `SendARP` for gateway MAC. No admin.
  - Optional feature `apple-wifi-extra` pulls CoreWLAN for transmit rate; we can do that ourselves.
- Alternatives: macOS `route -n get default` (shell, avoid); Windows `GetBestRoute2`/`GetIpForwardTable2` (IP Helper, no admin).
- **Recommendation:** `netdev` with default features; re-query on every path change event.

---

### 4. ICMP without root

#### macOS
- `socket(AF_INET, SOCK_DGRAM, IPPROTO_ICMP)` works for normal users. **[verified locally]**: uid 501, echo to 1.1.1.1 got a reply. On macOS the received datagram **includes the IPv4 header** (first byte 0x45), unlike Linux. Third-party docs say it has been allowed by default since 10.15 ([DataDog rshell ping](https://pkg.go.dev/github.com/DataDog/rshell@v0.0.22/builtins/ping), [ping-tokio](https://docs.rs/crate/ping-tokio/0.4.0)).
- `surge-ping` 0.9.1: `ConfigBuilder::sock_type_hint(socket2::Type::DGRAM)`; its `is_linux_icmp_socket!` macro treats non-Linux DGRAM as "IP header included", so macOS parsing is right; falls back to RAW on failure ([docs.rs ConfigBuilder](https://docs.rs/surge-ping/latest/surge_ping/struct.ConfigBuilder.html), [source](https://github.com/kolapapa/surge-ping/blob/master/src/client.rs)) **[verified in source]**. Not sandboxed, so no entitlement issue.

#### Windows
- Raw and DGRAM ICMP sockets need Administrator ([DataDog rshell ping](https://pkg.go.dev/github.com/DataDog/rshell@v0.0.22/builtins/ping)); surge-ping uses sockets → admin. Don't use it on Windows.
- `IcmpCreateFile` + `IcmpSendEcho2` (and `Icmp6*` for IPv6) from `Iphlpapi.dll`: supported for "desktop apps | UWP apps", no privilege requirement stated ([MS Learn](https://learn.microsoft.com/en-us/windows/win32/api/icmpapi/nf-icmpapi-icmpsendecho2)); this is what `ping.exe` uses as a normal user **[no-admin is inference from docs + common use]**. `RoundTripTime` is whole ms (fine for ≥1 ms; gateway RTT on Ethernet may read 0–1 ms). Feature `Win32_NetworkManagement_IpHelper`. Call synchronously on a blocking thread (`spawn_blocking`) with a timeout, or async with an Event.

#### Fallbacks (both OS)
- TCP connect timing: SYN → SYN-ACK (open) or RST (closed) both give one RTT; only silent drops time out ([BCC tcpconnlat](https://github.com/paranlee/bcc/blob/master/tools/tcpconnlat_example.txt), [xmodulo tcpping](https://www.xmodulo.com/how-to-install-tcpping-on-linux.html)). Routers often drop rather than RST on WAN-facing ports but usually answer on LAN 80/443/53 (admin UI/DNS) **[unverified, router-dependent]**. Try 53 → 80 → 443, record which port, 1 s timeout.
- Gateways can deprioritize ICMP (control-plane rate limiting) so a high gateway ping alone isn't proof of a bad router **[general knowledge]**; use median of several and compare to an external target.
- **Recommendation:** gateway = ICMP (mac DGRAM via surge-ping, win IcmpSendEcho2) → TCP connect fallback. External = TCP connect to 1.1.1.1:443 (no TLS) or ICMP to 1.1.1.1.

---

### 5. DNS resolution timing

- Configured servers without root:
  - `hickory-resolver` `system_conf::read_system_conf()`: macOS reads SCDynamicStore `State:/Network/Global/DNS` (`ServerAddresses`, `ServerPort`); Windows uses `ipconfig::get_adapters()` DNS of Up adapters ([apple.rs](https://github.com/hickory-dns/hickory-dns/blob/main/crates/resolver/src/system_conf/apple.rs), [windows.rs](https://github.com/hickory-dns/hickory-dns/blob/main/crates/resolver/src/system_conf/windows.rs)) **[verified in source]**.
  - `netdev` `Interface.dns_servers` (Windows GetAdaptersAddresses; macOS via its unix dns helper) **[verified in source]**.
  - macOS `/etc/resolv.conf` is an emulated, incomplete view (split DNS, scoped resolvers) ([resolver(5)](https://leancrew.com/all-this/man/man5/resolver.html), [c-ares note](https://git.gnunet.org/taler/quickjs-tart/file/subprojects/c-ares/src/lib/ares_sysconfig_mac.c.html)). Don't parse it.
- Two different measurements:
  1. **Resolver RTT**: one UDP A query straight to the configured server for a popular, certainly-cached name (e.g. the test host). Measures "is your DNS server slow/unreachable". Use `hickory-proto`/`hickory-resolver` with cache off, 1 attempt, 2 s timeout. ~70–150 bytes each way.
  2. **System lookup time**: `getaddrinfo`/`tokio::net::lookup_host` on a name. Hits mDNSResponder/DNS Client cache, often ~0 ms; only meaningful on a cache miss. A random-label name (`<rand>.example`) busts the cache but measures NXDOMAIN recursion to an authoritative server we don't own; avoid hammering third parties.
- Caveats: VPNs, iCloud Private Relay, configured DoH/DoT (Windows 11 DoH, macOS DNS profiles) mean the "configured server" may not be what the OS really uses; direct UDP/53 may be blocked by captive or corporate networks **[general knowledge]**. Report "DNS server didn't answer" separately from "names don't resolve".
- **Recommendation:** diagnosis = system lookup of the test host (did it resolve at all, how long) + direct resolver RTT to the first configured server. Flag DNS slow at >~200 ms median **[threshold is our choice]**.

---

### 6. External reachability probes (bytes measured locally with curl/openssl)

| Probe | Response | Measured | Notes |
|---|---|---|---|
| `http://cp.cloudflare.com/generate_204` | 204, empty | 813 B headers, 0 body | Plain HTTP, keep-alive; ≈1 KB with request + TCP. Good captive check (portal returns 200/30x). |
| `http://captive.apple.com/hotspot-detect.html` | 200 "Success" HTML | 69 B body, 390 B headers | Apple's own CNA probe; must be plain HTTP so portals can intercept ([purple.ai](https://www.purple.ai/guides/captive-portal-detection-cna-android-ncsi)). |
| `http://www.msftconnecttest.com/connecttest.txt` | 200 "Microsoft Connect Test" | 22 B body, 170 B headers | Windows NCSI active probe; also resolves `dns.msftncsi.com` ([MS Learn NCSI](https://learn.microsoft.com/en-us/troubleshoot/windows-client/networking/internet-explorer-edge-open-connect-corporate-public-network)). |
| `https://1.1.1.1/cdn-cgi/trace` | 200 text | 196 B body + 245 B headers; TLS handshake read 3709 B / wrote 1626 B | Body includes your public IP and `loc`; 16 fields (fl, h, ip, ts, visit_scheme, uag, colo, sliver, http, loc, tls, sni, warp, gateway, rbi, kex). Useful once per test for `colo` (server city); never log `ip`. |
| `https://www.cloudflare.com/cdn-cgi/trace` | same | 212 B + 914 B headers; TLS ≈3.9 KB read | |
| TCP connect `1.1.1.1:443`, then close | SYN/SYN-ACK/ACK/FIN | ≈300–400 B **[estimate]** | No HTTP, no TLS. Cheapest "internet reachable" signal. |

- A fresh HTTPS probe costs ≈6–7 KB (TLS 1.3 cert chain dominates). A minute budget of "a few KB" fits: 1 gateway ping (~100 B) + 1 TCP connect to 1.1.1.1 (~0.4 KB) + 1 DNS query (~0.2 KB) ≈ <1 KB/min. Run the HTTP captive probe only on path change or after a failure.
- Privacy: plain-HTTP probes go to Apple/Microsoft/Cloudflare. Brief allows only "our test servers + GitHub update check". So use **Cloudflare only** (`cp.cloudflare.com/generate_204`, `1.1.1.1`) for captive/reachability, and say so in docs/privacy. Don't call Apple/Microsoft endpoints.
- **Captive portal logic:** 204 → online; 200/30x with body → captive ("Sign in to this network"); timeout/RST → offline or blocked. Also on Windows `ConnectionProfile.GetNetworkConnectivityLevel()` (None/LocalAccess/ConstrainedInternetAccess/InternetAccess) is free OS knowledge **[unverified for unpackaged, likely fine]**; on mac `nw_path_get_status` gives satisfied/unsatisfied.

---

### 7. Background monitoring and battery

- macOS App Nap: candidates are apps not frontmost, not updating visible windows, not audible, without IOKit/NSProcessInfo assertions; effects are lower priority, **timer throttling**, I/O throttling ([Apple archive: App Nap](https://developer.apple.com/library/archive/documentation/Performance/Conceptual/power_efficiency_guidelines_osx/AppNap.html)). A tray app with a hidden popover is a candidate **[inference, LSUIElement not covered]**.
- Timers: set tolerance ≥10% of interval; avoid polling, prefer events; cancel timers you don't need; prefer `NSBackgroundActivityScheduler` for deferrable work ([Apple archive: Timers](https://developer.apple.com/library/archive/documentation/Performance/Conceptual/power_efficiency_guidelines_osx/Timers.html), [Scheduling](https://developer.apple.com/library/archive/documentation/Performance/Conceptual/power_efficiency_guidelines_osx/SchedulingBackgroundActivity.html)).
- **During a manual speed test** hold `NSProcessInfo.beginActivityWithOptions(.userInitiated, reason:)` so App Nap can't throttle the measurement when the popover closes mid-test; end it after. (objc2-foundation `NSProcessInfo`.)
- Windows: process QoS drops to Low when no window is visible; "Eco" QoS (EcoQoS) can be opted into with `SetProcessInformation(ProcessPowerThrottling, PROCESS_POWER_THROTTLING_EXECUTION_SPEED)` on Windows 11; user inactivity can lower foreground QoS on battery ([MS Learn QoS](https://learn.microsoft.com/en-us/windows/win32/procthread/quality-of-service)). Opt background-check threads into EcoQoS; switch the test thread back to HighQoS (or rely on the visible popover) during a manual test.
- Cadence **[our choice]**: every 60 s ±10% on AC; every 120–300 s on battery; pause on sleep, resume on wake and on path-change events; skip when `nw_path_get_status` / NetworkStatusChanged already says offline (log the outage edge from the event, not by polling). Never wake the webview for background checks; do them in Rust, emit only on state change.

---

### 8. Tauri 2 tray, positioner, window effects

- `tauri::tray::TrayIcon` (tauri 2.12.5): `set_title` (macOS; Linux needs an icon; **Windows unsupported**), `set_icon`, `set_icon_as_template` (macOS only), `set_tooltip`, `rect`, `set_show_menu_on_left_click` ([docs.rs](https://docs.rs/tauri/latest/tauri/tray/struct.TrayIcon.html)). Builder: `.icon_as_template(true)`. Margin already does `icon_as_template(cfg!(target_os = "macos"))` and `set_title` ([margin src-tauri/src/core/tray.rs](https://github.com/Abulkosim/margin/blob/main/src-tauri/src/core/tray.rs)). On Windows, put speed in the tooltip instead of a title.
- Windows tray colour: pick the icon from `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\SystemUsesLightTheme` (taskbar), **not** `AppsUseLightTheme` (apps); the mismatch made Claude Desktop's and Pritunl's tray icons invisible ([claudeissues 97697](https://claudeissues.com/issue/97697-desktop-windows-tray-icon-invisible-black-on-black-when-taskbar-is-dark-but-app), [Pritunl forum](https://forum.pritunl.com/t/bug-pritunl-windows-clients-system-tray-icon-color-is-broken-dark-icon-under-light-app-mode/2915)). Re-pick on `WM_SETTINGCHANGE` "ImmersiveColorSet". Margin already reads `SystemUsesLightTheme` (tray.rs:267). Ship white and black variants at 16/20/24/32 px for 100/125/150/200% DPI.
- Positioner plugin 2.4.0: feature `tray-icon`, forward events with `tauri_plugin_positioner::on_tray_event`, capability `positioner:default` ([Tauri docs](https://v2.tauri.app/plugin/positioner/)). Margin uses `Position::TrayCenter` + `move_window_constrained` on Windows and custom placement on mac; reuse it.
- Window effects (`tauri::window::Effect`) ([docs.rs](https://docs.rs/tauri/latest/tauri/window/enum.Effect.html)):
  - Windows: `Mica`/`MicaDark`/`MicaLight`, `Tabbed*` = Windows 11 only; `Acrylic` = Win10/11 but "bad performance when resizing/dragging" on Win10 1903+ and Win11 22000; `Blur` same issue on 22621. Popover doesn't resize-drag, so Acrylic is acceptable; Mica is safest on Win11. Fall back to solid on Win10.
  - macOS: `Popover`, `HudWindow`, `Menu`, `UnderWindowBackground`; new `LiquidGlassRegular`/`LiquidGlassClear` need macOS 26. Margin uses its own `NSVisualEffectView` via objc2 with `macOSPrivateApi: true` + `transparent: true`; reuse that.
  - `window-vibrancy` 0.8.1: `apply_mica` (Win11), `apply_acrylic`, `apply_blur`, `apply_vibrancy` (mac 10.10+) ([GitHub](https://github.com/tauri-apps/window-vibrancy)).
- Tray icon must be created on the main thread on macOS after the event loop starts ([tray-icon docs](https://docsearch.algolia.com/mcp/docs/repo/tauri-apps/tray-icon)) **[unverified source quality; Tauri handles this in setup]**.

---

### Permissions summary

| Capability | macOS | Windows |
|---|---|---|
| Metered / Low Data | none | none |
| Wi-Fi RSSI, noise, rate, channel | none **[verified]** | `current_connection` blocked without Location; bars/rssi-opcode/link speed likely free **[test]** |
| SSID / BSSID | Location prompt + Info.plist key; flaky with ad-hoc | Location prompt; we don't need it |
| Gateway IP + MAC | none | none |
| ICMP | none (DGRAM) **[verified]** | none via IcmpSendEcho2; sockets need admin |
| DNS servers + query | none | none |
| HTTP/TCP probes | none (not sandboxed) | none; first outbound may trigger nothing (outbound isn't firewalled by default) |

### Open items to test in the Windows VM
1. `GetSignalBars`, `WlanQueryInterface(wlan_intf_opcode_rssi)`, `GetConnectedSsid` without Location: do they succeed or prompt?
2. `GetConnectionCost` from an unpackaged exe; values on a phone hotspot.
3. `IcmpSendEcho2` as a standard user.
4. Tray icon contrast with light apps + dark taskbar.
5. Mica on Win11 popover; solid on Win10.

## Names and competitor complaints

Researched 2026-10-23. Unverified items are marked **(unverified)**.

### A. Names

#### How collisions were checked

- Homebrew: `https://formulae.brew.sh/api/cask/<name>.json` and `/formula/<name>.json` (404 = free).
- GitHub: `gh repo view Abulkosim/<name>` (all candidates free) and `gh search repos <name> --sort stars`.
- winget: blobless clone of [microsoft/winget-pkgs](https://github.com/microsoft/winget-pkgs), all `manifests/<l>/<Publisher>/<Package>` IDs grepped (13,531 IDs). Code search hit the API rate limit, so the tree listing was used instead.
- App Store: iTunes Search API (`itunes.apple.com/search?term=<name>&entity=macSoftware|software`), titles containing the word.
- Microsoft Store: no public search API; only web searches. **(unverified)** beyond "no network app with that name surfaced".

#### Brainstorm (all real English words, calm, measure/flow)

Weir, Readout, Sounding, Plumb, Ebb, Leeway, Tideline, Tidemark, Vane, Tally, Lull, Pace, Steady, Telltale, Lookout, Bearing, Plimsoll, Rill, Fathom, Baseline, Headroom, Gauge.

#### Collision table

| Name | Brew cask / formula | winget | GitHub | App Store | Verdict |
|---|---|---|---|---|---|
| **Weir** | free / free | none | [inconvergent/weir](https://github.com/inconvergent/weir) (639★, deprecated art lib), tidb-incubator/weir (96★) | no Mac app; iOS: "WEIR Pro", "NEXT by Weir" (Weir Group business apps), "Weir Open Channel Flow Calc" | **Keep** |
| **Readout** | free / free | none | nothing notable named `readout` (top hits are hardware/ML) | Mac: "Readout: Markdown Reader", "ReadoutIQ"; iOS: "Day Readout", "Race Readout". None network | **Keep** |
| **Sounding** | free / free | none | noisy ("human-sounding" TTS repos), no app named Sounding | Mac+iOS: "Soundings Mobile" (weather); iOS: "Grain, by Sounding" | **Keep** |
| Plumb | free / free | none | [Lv-0/plumb](https://github.com/Lv-0/plumb): macOS menu-bar window manager, [trending](https://trendshift.io/repositories/57768) | many "Plumb" finance/level/plumbing apps | Same shelf (mac menu bar on GitHub). Backup only |
| Ebb | free / free | none (`Ebbflow` publisher) | liyasthomas/ebb (88★), bigtreetech/EBB | ~25 iOS "Ebb" apps (health, tides) | Crowded; implies decline |
| Leeway | free / free | none | [gitpod-io/leeway](https://github.com/gitpod-io/leeway) (182★) | iOS **"Leeway - Your AI Usage Tracker"** (Margin's own category) | Reject |
| Tideline | free / free | none | estruyf/tideline: a **Tauri macOS menu-bar app** ([tomevault](https://tomevault.io/tome/estruyf/tideline)) | 8 iOS "Tideline" apps | Reject |
| Tidemark | free / free | none | [zbndev/tidemark](https://github.com/zbndev/tidemark): **AI quota monitor** (Margin's category) | 4 iOS apps | Reject |
| Vane | free / free | `GRCSAC.NetVane` | [ItzCrazyKns/Vane](https://github.com/ItzCrazyKns/Vane) (37.6k★ AI engine) | 3 iOS apps | Reject |
| Fathom | **cask taken** (Fathom video) | `Fathom.Fathom` | many | many | Reject |
| Baseline | **cask taken** (SecondSonConsulting/Baseline) | none | many | — | Reject |
| Headroom | **cask taken** ("Reduce token usage for Claude Code and Codex") | `M-Igashi.headroom` | — | — | Reject |
| Gauge | free / **formula taken** | `Gauge.Gauge` | getgauge | — | Reject |
| Tally | free / free | `Wharflab.Tally` | uber-go/tally (909★) | 60+ apps | Reject |
| Steady | free / free | `Steady.Steady` | — | — | Reject |
| Pace | free / free | `Azuyamat.Pace` | — | dozens (running) | Reject |
| Lull | free / free | none | — | ~30 sleep apps | Reject (also means "drop") |
| Telltale | free / free | `sanlee-ys.telltale` | — | Telltale Games brand | Reject |
| Lookout | free / free | none | — | Lookout (mobile security co.) | Reject |
| Rill | free / free | none | [rilldata/rill](https://github.com/rilldata/rill) (2.9k★) | — | Reject |

Web searches for "Weir", "Readout", "Sounding" + speed/network/menu bar app found no same-name network tool ([search 1](https://alternativeto.net/feature/network-speed-monitor/), menu-bar results were NetTop, Flux Network Monitor, NetPulse, Net Bar). Microsoft Store: **(unverified)**.

#### Top 3

1. **Weir** — a weir is the low wall across a stream that steadies it and measures its flow. 4 letters, calm, a plain noun like "Margin". Cleanest result: no cask/formula/winget, no Mac app, no active GitHub project. Risks: Weir Group PLC (industrial pumps; trademark in a different class), may be misread as "weird", obscure to ordinary people.
2. **Readout** — "the reading on an instrument". Everyone gets it; fits "honest numbers". Brew/winget clean; App Store has a Markdown reader and a few unrelated "Readout" apps. Risks: "read out loud" sense; less distinctive; 7 letters.
3. **Sounding** — a depth measurement taken from a boat; a log of soundings is exactly the quiet history. Brew/winget clean. Risks: reads as audio; "Soundings Mobile" weather app exists on Mac.

Backup: **Plumb** (plumb line = true measure), if the owner accepts sharing a word with a trending mac window-manager repo.

### B. Competitor complaints

Sources: App Store review RSS feeds fetched 2026-10-23 (US store, most recent 250 per app): [Speedtest iOS](https://apps.apple.com/us/app/speedtest-by-ookla/id300704847) ([feed](https://itunes.apple.com/us/rss/customerreviews/id=300704847/sortBy=mostRecent/json)), [Speedtest Mac](https://apps.apple.com/us/app/speedtest-by-ookla/id1153157709) ([feed](https://itunes.apple.com/us/rss/customerreviews/id=1153157709/sortBy=mostRecent/json)), [iWifi](https://apps.apple.com/app/id1476136371), [Orb](https://apps.apple.com/app/id6477840170). Hacker News via Algolia. Reddit is not reachable from this agent's tools, so no Reddit citations.

#### 1. Ads, trackers and data harvesting (Ookla)

- iOS reviews: "you MUST agree to personalized ad trackers being installed", "Main purpose of app is selling your data … The better alternative … is speed.cloudflare.com", "App has explicit ads … rated 4+", "Privacy? You really need to steal all our data to run a speed test?" ([iOS reviews](https://apps.apple.com/us/app/speedtest-by-ookla/id300704847)).
- Apple privacy label lists location, identifiers and usage data used to track ([listing](https://apps.apple.com/ca/app/speedtest-internet-wifi-test/id300704847)). Older Exodus report: 8 trackers incl. AdMob, Amazon ads ([Exodus](https://reports.exodus-privacy.eu.org/en/reports/74481), 2019–20 version).
- Ownership changed: Accenture agreed to buy Ookla from Ziff Davis for ~$1.2B, March 2026 ([ISPreview](https://www.ispreview.co.uk/index.php/2026/03/broadband-speed-testing-giant-ookla-acquired-by-accenture.html)); reviewers cite it as their reason to delete.
- **Us:** no ads, no trackers, no account, no location. Say it in one line in Settings → About and on the site. Contact only the test server and GitHub.

#### 2. Accounts, logins and bloat

- Mac reviews ask to log in to sync history; iOS reviews say benefits require an account and its ToS ([Mac](https://apps.apple.com/us/app/speedtest-by-ookla/id1153157709), [iOS](https://apps.apple.com/us/app/speedtest-by-ookla/id300704847)). Orb "logs you out with every update" ([Orb](https://apps.apple.com/app/id6477840170)).
- "Terrible redesign, bloated UI … Give us the option to disable the articles"; Downdetector folded in and rarely loads ([iOS](https://apps.apple.com/us/app/speedtest-by-ookla/id300704847)).
- **Us:** history stored locally with no account. One job per screen. No news feed, no upsell.

#### 3. Battery and CPU drain from tray/menu-bar apps

- Ookla Mac: "drains MacBook battery like mad … under 70% in half an hour", "Energy impact of 2494 in the last 12 hours", "constant CPU usage and made the fans come on", "drags CPU in background" ([Mac reviews](https://apps.apple.com/us/app/speedtest-by-ookla/id1153157709)). Earlier: 1–2% CPU idle ([TidBITS 2017](https://tidbits.com/2017/01/16/run-ookla-speedtest-from-your-menu-bar/)).
- Orb: "no run in background option … Unacceptable overhead" ([Orb](https://apps.apple.com/app/id6477840170)).
- **Us:** idle means idle: no webview rendering when hidden, no timers faster than the light check. Measure idle CPU/energy and record it in `docs/verification.md` per release.

#### 4. Data use, especially on cellular

- "it took 1.2GB quickly … other speed test only takes 100Mb … I only recommend this on WiFi" ([iOS review](https://apps.apple.com/us/app/speedtest-by-ookla/id300704847)).
- Menu-bar competitors advertise automatic speed tests every 5 min to 1 h ([Flux Network Monitor](https://apps.apple.com/app/id6780369565)), which can be gigabytes per day **(our inference, not a cited complaint)**.
- **Us:** show "Uses about N MB" before every test, cap bytes per test, never auto-run full tests on metered/hotspot/low-data, background checks a few KB per minute.

#### 5. "Why does every site give a different number?"

- Forum: "every speed test was different … even with the closest servers" ([SevenForums](https://www.sevenforums.com/threads/question-about-accurate-speed-testing-and-choke-points.75027/post-656770)); ISP explainer on servers, method, time of day ([Spintel](https://articles.spintel.net.au/article/why-speed-test-results-differ-factors-behind-varied-speed-test-results.html)); Mac review: "Not sure how accurate this is" vs the ISP's own test ([Mac reviews](https://apps.apple.com/us/app/speedtest-by-ookla/id1153157709)); Orb "way lower than anywhere else" ([Orb](https://apps.apple.com/app/id6477840170)).
- Causes are documented: single vs multi-stream and off-net servers ([M-Lab](https://support.measurementlab.net/help/en-us/6-measurement-services/11-why-are-my-m-lab-results-different-from-other-speed-tests), [M-Lab blog](https://www.measurementlab.net/blog/speed-tests-accuracy/)); fast.com reads lower when its server is ~70 ms away ([NANOG 2017](https://mailman.nanog.org/pipermail/nanog/2017-June/091602.html)).
- **Us:** always show server, city and time next to the number. "About this number" sheet: how many streams, how long, which server. Never a "peak".

#### 6. Speed tests that look fine while the connection feels bad (gaming the test, bufferbloat)

- HN: "ISPs are definitely shaping traffic to try to game results … large latency spikes for ~2 hours every night, and speedtest.net always remained at normal latency" ([HN](https://news.ycombinator.com/item?id=11724519)); "fast.com and speedtest.net are unfortunately not useful" for proving a Comcast problem ([HN](https://news.ycombinator.com/item?id=36754339)).
- Documented: Ookla found a UK ISP prioritising port 8080 used by its own speed tester ([ISPreview 2016](https://www.ispreview.co.uk/index.php/2016/01/ookla-accuses-uk-broadband-isp-manipulating-internet-speedtests.html)).
- Bufferbloat is invisible to throughput tests ([Wikipedia](https://en.wikipedia.org/wiki/Bufferbloat), [OpenWrt forum](https://forum.openwrt.org/t/2025-current-recommendations-for-bufferbloat-testing/241303/1)).
- **Us:** loaded latency is a headline number ("Latency 14 ms · 62 ms under load"). The light background check and outage timeline catch the evening-only problem a single test misses.

#### 7. Numbers without meaning

- Orb: "don't know what's the meaning of the numbers"; "what am I supposed to do with any of this info when I know my internet is slow" ([Orb](https://apps.apple.com/app/id6477840170)).
- Ookla gauge capped at 500 Mbps on Mac for years ([Mac reviews](https://apps.apple.com/us/app/speedtest-by-ookla/id1153157709)), a sign the visual matters more than the meaning.
- **Us:** one plain sentence per result ("Good for 4K video and calls"), and the Wi-Fi → router → DNS → internet diagnosis as sentences, not charts.

#### 8. Paywalls and subscriptions for basic utilities

- iWifi: "Immediate Pay Wall", "$4 a week", "charged after I cancelled", "Paid version and still get ads" ([iWifi](https://apps.apple.com/app/id1476136371)). Ookla iOS: "Not Free … Now they want you to pay" ([iOS](https://apps.apple.com/us/app/speedtest-by-ookla/id300704847)).
- Pro tools cost: PingPlotter $6.99–$29/month, reviewers want it at "1/5 of the price" ([Capterra](https://www.capterra.com/p/143523/PingPlotter/pricing/)); iStat Menus 7 is a paid upgrade ([Bjango FAQ](https://bjango.com/help/istatmenus7/update/)). Complexity complaints about PingPlotter are rare in reviews (it is rated easy), so "too complex" is **(unverified)** for it.
- **Us:** free, MIT, no tiers. Say so on the site's first screen.

#### 9. Limited or developer-only tools (fast.com, networkQuality, M-Lab)

- fast.com: tests only Netflix's Open Connect servers; upload/latency hidden behind "more info"; ISP-specific Netflix throttling skews it ([HN launch thread](https://news.ycombinator.com/item?id=11723035), [pong.com](https://www.pong.com/blog/why-speedtest-net-faster-than-fast-com)). "fast.com has anycast nodes on most major ISPs so they can be misleading" ([HN](https://news.ycombinator.com/item?id=36757873)).
- networkQuality: praised ([HN 2021](https://news.ycombinator.com/item?id=29225308), [HN 2023](https://news.ycombinator.com/item?id=35936999)) but Terminal-only; people build Shortcuts/SwiftBar wrappers or use DEVONtechnologies' Network Utility ([MacMost](https://macmost.com/test-your-network-speed-on-a-mac.html), [Six Colors](https://sixcolors.com/?p=21743), [TidBITS](https://tidbits.com/2024/08/07/devontechnologies-resurrects-network-utility/)).
- Google/M-Lab: single stream reads lower ([M-Lab](https://support.measurementlab.net/help/en-us/6-measurement-services/11-why-are-my-m-lab-results-different-from-other-speed-tests)); test publishes your IP in an open dataset ([Google help](https://support.google.com/websearch/answer/6283840), [M-Lab privacy](https://www.measurementlab.net/privacy/)).
- **Us:** fast.com-simple in the tray, with loaded latency and upload shown by default. If M-Lab is offered, warn about IP publication before the first test.

#### 10. Stale, broken or network-switch-unaware apps

- Ookla Mac: "hasn't been updated in 3 years", broken on macOS 27 betas; "As soon as you switch networks, it claims you have no Internet access" ([Mac reviews](https://apps.apple.com/us/app/speedtest-by-ookla/id1153157709)).
- **Us:** react to path changes (new network clears "stale" results, re-checks); in-app updater; release discipline as in Margin.
