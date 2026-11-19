# Method

How Weir measures a connection. Evidence and sources: [research.md](research.md). Every rule here maps to a pure function in `src-tauri/src/core/` with unit tests on recorded sample fixtures.

## Goals

- One number per metric that the link **sustains**, not a peak.
- Whole test about 12–20 s, 21 s at most.
- Bounded data: 1 GB ceiling per test, estimate shown before it starts.
- A test that fails or is cut short shows no number for that metric, and says why.

## Server

Cloudflare's speed test endpoints, the same ones speed.cloudflare.com uses:

| Request | Use |
| --- | --- |
| `GET https://speed.cloudflare.com/cdn-cgi/trace` | Pre-flight. Read only `colo` (IATA code of the data center). |
| `GET https://speed.cloudflare.com/__down?bytes=0` | Latency probe. |
| `GET https://speed.cloudflare.com/__down?bytes=N` | Download, `N` ≤ 25 MB. |
| `POST https://speed.cloudflare.com/__up` | Upload, body ≤ 25 MB. |

- HTTP/1.1 only, one TCP connection per stream, so streams are real parallel connections.
- `User-Agent: Weir/<version> (+https://github.com/Abulkosim/weir)`. No Referer or Origin spoofing.
- Never POST results anywhere. Never store response headers other than `colo`; the rest (`cf-meta-ip`, city, ASN, coordinates) is the user's identity.
- `colo` maps to a city through a bundled IATA table: "Cloudflare, Frankfurt". Unknown code → "Cloudflare, FRA".
- 403/429/5xx: stop, show "Cloudflare refused the test" or "Cloudflare server error". Honour `Retry-After` before allowing a retry. No silent retries.

## Sequence

### 0. Pre-flight (≤ 5 s, ~10 kB)

`cdn-cgi/trace` → colo. Fails in 5 s → "No internet" (offline state), nothing else runs.

### 1. Idle latency (~2 s, < 50 kB)

- One probe connection, kept open for the whole test. The 6 download connections open in parallel now (pre-warm).
- 20 probes, one at a time, ≥ 50 ms apart, 2 s timeout each. First probe dropped (connection warm-up).
- RTT = time to first byte − server time. Server time = `cfReqDur;dur=` from `Server-Timing` if present, else the sum of `cfSpeed*;dur=`. Header missing or result ≤ 1 ms → plain TTFB.
- **Latency = median.** **Jitter = mean |Δ| of consecutive RTTs** (Cloudflare's formula).
- Valid with ≥ 10 answered probes. Else latency and jitter show "Not measured"; speed may still be valid.

### 2. Download (5–10 s, ≤ 570 MB)

- **Streams:** 6 (8 when idle latency > 100 ms). Each stream loops requests.
- **Request size:** first 256 kB, then `clamp(stream_rate × T, 256 kB, 25 MB)`, so each request takes about T = `clamp(16 × idle latency, 1 s, 2 s)`. A stream waits a round trip between requests; at 120 ms that would be a fifth of a short request, so far links get longer ones. Random query parameter defeats caches.
- **Sampling:** one atomic byte counter for all streams, read about every 100 ms on a monotonic clock. Readings are interpolated onto 250 ms bins, so timer jitter doesn't skew samples. The UI gets a 1 s moving average at 10 Hz.
- **Warm-up discard:** the steady window starts at the first 250 ms sample where the 1 s moving average stops rising (< 3 % above its previous maximum for 2 samples in a row), no earlier than 1 s and no later than 4 s. Starting at 4 s marks the result "ramped slowly".
- **Stop** at the first of:
  - steady window ≥ 3 s, total ≥ 5 s, and the running mean of the last 6 steady samples moved < 3 %;
  - 10 s total;
  - 570 MB received. Streams take each request's size from what is left of the cap, so together they stop at it instead of overshooting until the next 100 ms reading.
  On stop, all streams are dropped. Only bytes received before the stop instant count.
- **Result = steady-window mean** = steady bytes × 8 ÷ steady seconds.
- Also stored: p10, p50, p90 of steady 250 ms samples. When (p90 − p10) ÷ mean > 25 %, the result says "Varied 280–330 Mbps". The maximum is never shown.

### 3. Loaded latency (during download and upload)

- The probe connection from step 1, separate from the load (IETF "foreign" probe).
- One probe every 250 ms. Only probes **sent inside the steady window** count.
- A probe that times out (2 s) counts as 2000 ms. Dropping it would hide bufferbloat.
- Median per direction, ≥ 5 counted probes, else "Not measured".
- **Under load = max(download median, upload median).** "Latency 14 ms · 62 ms under load".

### 4. Upload (5–8 s, ≤ 350 MB)

- 300 ms pause after download so queues drain.
- **Streams:** 4 (6 when idle latency > 100 ms). `POST __up`, body `clamp(stream_rate × T, 32 kB, 25 MB)` with T = `clamp(16 × idle latency, 0.5 s, 2 s)` from one static zero buffer. The first request is 32 kB, not download's 256 kB: a whole request lands in the socket send buffer at once, so the byte counter moves only when one completes, and four 256 kB requests take 8 s on a 1 Mbps uplink, past the stall rule (docs/accuracy.md, F-2). The result needs a steady window of at least 1 s that every stream has finished a request in.
- **Live gauge:** bytes written to the sockets, 1 s moving average.
- **Result:** bytes written to the socket overstate delivery (send buffers hold MBs), so the result comes from completed requests. Each request's bytes are spread over its transfer time (response TTFB − server time). Result = bytes carried inside the steady window ÷ the window's wall time. The window ends where the first stream's last completed transfer ends, so no request is still in flight inside it. The time a stream waits between requests counts: adding per-stream rates instead read above what the link carried (docs/accuracy.md, F-5).
- Warm-up and stop rules are the same as download, applied to the socket counter, which is good enough to see the plateau. A steady window under 1 s, or a stream with no finished request in it → the result falls back to the counter mean and is marked "rough".
- Download failed → upload is skipped.

## Validity

A phase shows no number unless all hold:

| Rule | Message |
| --- | --- |
| Steady window ≥ 2 s (≥ 1 s when the byte cap stopped it) | "Test too short to measure" |
| No 3 s with zero bytes on every stream | "Connection stalled" |
| ≥ 50 % of streams alive through the steady window (one restart per stream allowed) | "Connection kept dropping" |
| Every response is 200 | "Cloudflare refused the test" / "Cloudflare server error" |
| No network change, sleep or wake during the test | "Network changed during the test" |
| Not cancelled | "Cancelled" (logged in history, no numbers) |

Byte cap reached with ≥ 1 s steady → valid, labelled "Stopped at the data limit, speed may be higher".

## Packet loss

Not reported in v0.1. Cloudflare's loss test needs a TURN server; ICMP loss misreads because routers deprioritise ICMP. Probes that don't answer are shown as they are: "3 of 20 checks didn't answer".

## Data use

- Ceiling: 570 MB down + 350 MB up + 1 MB probes, plus 8 % for IP and TCP headers and the other direction's ACKs ≈ **1 GB**. It was 600 MB until the accuracy lab showed that a 1 Gbps upload hit its 190 MB cap before it had a steady second, and that a far gigabit link was still ramping when its caps ended the test (docs/accuracy.md, F-1 and F-4); it also measured the old 5 % overhead allowance as too small (F-3).
- **Estimate before a test:** `E = (min(570, R_d/8 × 7) + min(350, R_u/8 × 6) + 1) × 1.08` MB, where `R` is the median of the last 3 results (Mbps). No history, or a metered link (whose recent results may come from a slower network) → "Uses up to 1 GB". Rounded up to 5 MB below 50 MB, else up to 10 MB, so the estimate never understates.
  - 100/20 Mbps → about 120 MB. 300/40 → about 320 MB. 1000/100 → about 700 MB.
- After the test: actual bytes counted, "Used 212 MB".
- Metered, hotspot or Low Data Mode → the Test button asks first: "You're on a hotspot. Test anyway? Uses about 110 MB".

## Plain meaning

Our own rules, built from Netflix, Zoom, Teams, IETF responsiveness bands and Cloudflare AIM boundaries ([research](research.md#3-what-this-speed-is-good-for--authoritative-thresholds)).

| Topic | Rule | Copy |
| --- | --- | --- |
| Video | down ≥ 25 | "Good for 4K video" |
| | 15–25 | "4K on one screen" |
| | 5–15 | "Good for HD video" |
| | 3–5 | "720p video" |
| | < 3 | "Video may buffer" |
| Calls | up ≥ 4, down ≥ 4, under load < 100 ms, jitter < 30 ms | "Good for video calls" |
| | under load 100–200 ms | "Calls may stutter: latency jumps to N ms under load" |
| | under load ≥ 200 ms | "Calls will lag when the line is busy" |
| | up < 1.5 | "Upload too slow for video calls" |
| Games | idle < 20 and increase < 20 | "Good for competitive games" |
| | idle < 50 and increase < 50 | "Fine for online games" |
| | else | "Online games may lag" |
| Load | increase < 30 ms | "Steady under load" |
| | 30–100 ms | "Slows under load" |
| | ≥ 100 ms | "Lags under load" |

Increase = under load − idle. The headline sentence picks the most limiting line: "Good for 4K video and calls", or the first problem.

## Your plan

Optional, in Settings → Your plan: the download and upload speeds the provider sells, whole Mbps.

- Share = measured ÷ plan, rounded to a whole percent. Over 100 % is shown as it is.
- ≥ 80 % good, 50–79 % fair, < 50 % poor. Many regulators and providers treat about 80 % of the advertised speed as normal at busy times, so we don't call less than that good.
- A test that stopped at its data limit says "At least N %" and is never called poor: the line may be faster than we measured.
- Shown under the download number, under Upload, in History (median of recent tests) and in the tray tooltip. It never changes the plain-meaning verdict, which is about what the line is good for, not what it costs.
- Wi-Fi often caps a fast plan. A low share on Wi-Fi is a reason to test on a cable before calling the provider.

## This month

The first card in History ([`month.ts`](../src/lib/month.ts)):

- The calendar month in local time. Download, upload and idle latency are medians of that month's results, like everything else here; an average would let one bad test drag the number.
- Offline time adds up outages that overlap the month, counting only the part inside it. With the outage watch off and nothing recorded it says "Outage watch is off", not "No outages".
- Failed tests are counted, never averaged in. Last month's median download is shown beside it to compare.
- Until the new month has a test, the card shows last month under its name, so the 1st doesn't open on an empty card.

## Outage watch

Off by default. Settings → Outages → Watch for outages ([`monitor.rs`](../src-tauri/src/monitor.rs), [`outage.rs`](../src-tauri/src/core/outage.rs)).

- About once a minute (60 s ± 10 %), one tiny request to Cloudflare on a kept-alive connection. Two failed checks in a row start an outage, dated from the first. The OS reporting no network path starts one at once. A gap of 5 minutes or more between checks means the computer slept: an open outage closes at the last check before the gap.
- **Where it broke.** Each failed check, including the first one before the outage starts, is classified:

  | The OS reports | The router | The DNS server | Counted as |
  |---|---|---|---|
  | no network path (Wi-Fi off, cable out) | – | – | no network |
  | a path | answered | not asked | past your router |
  | a path | silent | answered, and it is the router or past it | past your router |
  | a path | silent | silent, none, or on the home network | at your router |
  | a path | not asked: no gateway, IPv6 only, a public gateway (the provider's), VPN or other tunnel, cellular, or out of time | – | unclear |
  | a path that changed while the check ran | – | – | unclear |

- A DNS server elsewhere on the home network (a Pi-hole, a NAS) or on this computer can answer while the router is down, so its reply proves nothing and it isn't asked. VPN adapters are found by type, and by name for those that report as plain Ethernet (TAP, Wintun, WireGuard, AnyConnect and similar).
- The router gets 2 probes (ICMP, or TCP to ports 80, 443 and 53 where ICMP isn't allowed), not the 5 of **Check connection**. The DNS server is asked only when the router is silent, at most twice, and any reply counts, even "server failure": a router forwarding DNS during an upstream outage still answers. Each stage has its own time limit (1 s, 5 s, 3.5 s); a limit hit reads as unclear, never as silence.
- Only one router probe runs at a time, and on macOS only replies from the router's address count, so **Check connection** or a `ping` in Terminal can't be mistaken for the router answering. A watch probe waits up to 12 s for one already running (unclear if it can't start), and a probe cut short keeps the next one waiting until its ICMP socket is closed.
- "No network path" means no usable interface at all. On Windows that is connectivity level None; LocalAccess, which Windows reports when the provider is down, still counts as a path, so the router and DNS server are asked instead of blaming this computer.
- OS path updates arrive in bursts. Each spell without a network counts as one "no network" check, however many updates it sends, even after a failed check: a Wi-Fi drop right after one failed check is never put down as past the router. Minute checks that find no network count each.
- **Router blame needs proof.** A router can ignore pings, so silence alone is weak. When the internet is back, Weir asks the router once more: router blame stands only if it answers then, on the same network, without a sleep in between. If it stays silent, the network changed, or the outage closed across a sleep, those checks become unclear. An answer that began before the OS lost every path doesn't end the outage; the next check decides.
- **Verdict** for History, the notification, the report, the CSV and `weir history`, over all checks of the outage (unclear included):
  - every check in one place: "Your router was reachable throughout. The break was past it."
  - more than half in one place: "Mostly past your router: 5 of 7 checks."
  - one place, the rest unclear: "Past your router in 3 of 7 checks. The rest were unclear." This is no verdict: the report counts it under "Not known" and `weir history` gives `cause: null`.
  - more than one place, none over half: "Checks disagreed: 3 of 7 at your router, 3 past it, 1 unclear."
  - only unclear checks: nothing is said.
- History keeps only the four counts, never the router or DNS address, Wi-Fi name or IP. History files stay version 1: Weir 0.7.3 and earlier ignore the counts, so a downgrade keeps working and only loses where outages broke.
- **Data per check.** Computed, not measured on the wire:

  | Packet | Size | When |
  |---|---|---|
  | ICMP echo (each way) | 44 B on macOS, 45 B on Windows | 2 per failed check |
  | TCP SYN | about 64 B, up to 2 per port with a retransmit, so ≤ 12 SYNs ≈ 0.8 kB | only if ICMP gets no answer or isn't allowed |
  | DNS query / reply | 66 B / about 150 B at most | only if the router is silent, ≤ 2 |

  A failed check where the router answers adds about 0.2 kB; the worst case sends about 1 kB. The re-check after an outage blamed on the router is about 0.2 kB, once. ARP broadcasts stay on the local network and aren't counted. With the failed Cloudflare request, a minute stays under 2 kB.

## Provider report

**Report** in History (or Settings → History → Save PDF) writes an A4 PDF of the last 30 days, built in Rust ([`report.rs`](../src-tauri/src/core/report.rs)) with the PDF's built-in Helvetica, so no fonts are embedded and nothing is fetched.

- Header: the period, the UTC offset of every time, and the plan if one is set.
- Medians for download, upload and latency (idle and under load), the download and upload range, the median's share of the plan, and how many tests failed.
- Plain findings: tests under half the plan, the slowest hours (same rule as History: at least 10 tests over 3 hours of the day, hours under 80 % of the usual median), outages with the longest one and where they broke ("Where it broke: past your router in 3, at your router in 1. Not known for 2."), and failures. If the outage watch is off the report says outages were not monitored, instead of implying there were none.
- A chart of every download over the 30 days with the plan as a dashed line, tests under half the plan in red and outages as red marks on the baseline.
- Every test, newest first, with failures and outages in place (for example "No internet until 14:09 · 7 min · past your router"), and a short note on how Weir measures.
- No IP address, Wi-Fi name or location; the server is the Cloudflare data-centre code, as in History.

## Scheduled tests

Off by default. Settings → Scheduled tests: every 6 hours, every 12 hours or once a day ([`schedule.rs`](../src-tauri/src/core/schedule.rs)).

- The same full test as the Test button, so results compare. Marked as scheduled in History and the CSV notes, and counted in the provider report.
- Runs only when the connection has been suitable for 2 minutes: online, no outage, not metered (hotspot, Low Data Mode, cellular, roaming, Windows metered cost), not in Low Power Mode or Battery Saver, and no test or connection check running. Waking from sleep or logging in starts the 2 minutes again.
- Due when no test, by hand or scheduled, ran within the interval. A cancelled scheduled test is not retried until the next interval.
- A scheduled test never asks to confirm data use, so it never runs where the Test button would ask. A failure goes into History without replacing the last result in the popover.
- Settings shows the cost before you choose: the per-test estimate × tests a month, for example "About 250 MB a test, 15 GB a month" at every 12 hours.

## Comparing with other tests

Weir uses 6 parallel streams and a sustained mean. speed.cloudflare.com uses one stream at a time and the 90th percentile of per-request speeds. On links up to ~300 Mbps with low latency the two should agree within about 10 %. On gigabit or high-latency links Weir may read higher because parallel streams fill the link faster. This is checked by hand on the owner's Mac and recorded in [verification.md](verification.md).

On shaped links the accuracy lab checks Weir against the wire, iperf3 and Cloudflare's own test library on every engine change: [accuracy.md](accuracy.md).
