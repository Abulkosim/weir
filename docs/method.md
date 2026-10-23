# Method

How Weir measures a connection. Evidence and sources: [research.md](research.md). Every rule here maps to a pure function in `src-tauri/src/core/` with unit tests on recorded sample fixtures.

## Goals

- One number per metric that the link **sustains**, not a peak.
- Whole test about 12–20 s, 21 s at most.
- Bounded data: 600 MB ceiling per test, estimate shown before it starts.
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

### 2. Download (5–10 s, ≤ 400 MB)

- **Streams:** 6 (8 when idle latency > 100 ms). Each stream loops requests.
- **Request size:** first 256 kB, then `clamp(stream_rate × 1 s, 256 kB, 25 MB)`, so each request takes about 1 s. Random query parameter defeats caches.
- **Sampling:** one atomic byte counter for all streams, read every 100 ms on a monotonic clock, grouped into 250 ms samples. The UI gets a 1 s moving average at 10 Hz.
- **Warm-up discard:** the steady window starts at the first 250 ms sample where the 1 s moving average stops rising (< 3 % above its previous maximum for 2 samples in a row), no earlier than 1 s and no later than 4 s. Starting at 4 s marks the result "ramped slowly".
- **Stop** at the first of:
  - steady window ≥ 3 s, total ≥ 5 s, and the running mean of the last 6 steady samples moved < 3 %;
  - 10 s total;
  - 400 MB received.
  On stop, all streams are dropped. Only bytes received before the stop instant count.
- **Result = steady-window mean** = steady bytes × 8 ÷ steady seconds.
- Also stored: p10, p50, p90 of steady 250 ms samples. When (p90 − p10) ÷ mean > 25 %, the result says "Varied 280–330 Mbps". The maximum is never shown.

### 3. Loaded latency (during download and upload)

- The probe connection from step 1, separate from the load (IETF "foreign" probe).
- One probe every 250 ms. Only probes **sent inside the steady window** count.
- A probe that times out (2 s) counts as 2000 ms. Dropping it would hide bufferbloat.
- Median per direction, ≥ 5 counted probes, else "Not measured".
- **Under load = max(download median, upload median).** "Latency 14 ms · 62 ms under load".

### 4. Upload (5–8 s, ≤ 200 MB)

- 300 ms pause after download so queues drain.
- **Streams:** 4 (6 when idle latency > 100 ms). `POST __up`, body `clamp(stream_rate × 1 s, 256 kB, 25 MB)` from one pre-generated buffer.
- **Live gauge:** bytes written to the sockets, 1 s moving average.
- **Result:** bytes written to the socket overstate delivery (send buffers hold MBs), so the result comes from completed requests. Per stream, requests that started inside the steady window: Σ bytes ÷ Σ (response TTFB − server time). Total = sum over streams. `cf-meta-upload-bytes` cross-checks the count.
- Same warm-up and stop rules as download, applied to completed-request rates. Fewer than 3 completed requests per stream → fall back to the counter and mark "rough".
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

- Ceiling: 400 MB down + 200 MB up + < 1 MB probes ≈ **600 MB**.
- **Estimate before a test:** `E = (min(400, R_d/8 × 7) + min(200, R_u/8 × 6) + 1) × 1.05` MB, where `R` is the median of the last 3 results on this network (Mbps). No history → "Uses up to 600 MB". Rounded to 5 MB below 50 MB, else to 50 MB.
  - 100/20 Mbps → about 110 MB. 300/40 → about 300 MB. 1000/100 → about 500 MB.
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

## Comparing with other tests

Weir uses 6 parallel streams and a sustained mean. speed.cloudflare.com uses one stream at a time and the 90th percentile of per-request speeds. On links up to ~300 Mbps with low latency the two should agree within about 10 %. On gigabit or high-latency links Weir may read higher because parallel streams fill the link faster. This is checked by hand on the owner's Mac and recorded in [verification.md](verification.md).
