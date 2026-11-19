// Gate thresholds, pre-registered before the first full run. Each carries the
// reason it has this value. Changing one needs a reason that cites observed lab
// noise; docs/accuracy.md prints every reason.

export interface Threshold {
  reason: string;
  [limit: string]: number | string;
}

export const THRESHOLDS = {
  downloadWire: {
    maxAbsPct: 3,
    reason: "Same run, same window: the only gap is app-versus-kernel buffering at the window edges plus Weir's 100 ms tick interpolation.",
  },
  uploadWire: {
    maxAbsPct: 5,
    reason: 'Weir spreads each completed request evenly over its transfer, and the wire is counted over the same window Weir used. Bursts inside a request and the half round trip between client and server timing add a few percent.',
  },
  capacityClean: {
    medianMinPct: -10,
    medianMaxPct: 5,
    worstAbsPct: 20,
    reason: "docs/method.md promises agreement within about 10 %. Over-reading is held tighter because iperf3 already sits near the physical ceiling.",
  },
  capacityNoisy: {
    medianAbsPct: 20,
    worstAbsPct: 35,
    reason: 'Loss-limited TCP is high-variance, and HTTP requests recover from tail loss differently from one long bulk transfer. Five repeats, not a wider band, carry the noise.',
  },
  physics: {
    factor: 1.02,
    reason: 'No run may report more than the shaped rate can carry (payload per frame, 1448/1514), plus 2 % for tbf burst and window-edge effects.',
  },
  idle: {
    absMs: 2,
    pct: 10,
    reason: 'An HTTP probe and an ICMP echo cross the same path; user-space scheduling adds well under 2 ms on an idle host.',
  },
  idleJitter: {
    absMs: 3,
    pct: 15,
    reason: "On a jittery path Weir's 19 probes and the concurrent ping sample different packets of the same delay distribution, so each median carries its own sampling noise. Judged every repeat against the ping taken during Weir's own latency phase: the widest gap in the four runs before this rule was 1.7 ms on wifi-jitter and 3.7 ms on mobile.",
  },
  loaded: {
    absMs: 10,
    pct: 15,
    reason: "Per run. Weir's probes are closed-loop (each waits for the last, every 250 ms at most) and the ping is open-loop at 50 ms, so on a deep queue the two sample different moments of the same sawtooth; a larger gap is Weir's probe method or its runtime scheduling.",
  },
  loadedBias: {
    absMs: 5,
    pct: 7.5,
    reason: 'The median of the signed per-run errors, against the median ping. Half the per-run band: sampling noise averages out over repeats, a sampling bias does not. Catches Weir under-reporting bufferbloat while every run stays inside its own band. Report only since the run after the F-2 fix: across six runs the bias on deep-buffer links moved by more than its own limit between runs and changed sign, which points at the ping as much as at Weir, since a full tail-drop queue drops pings at its fullest moments. It gates again once the lab records ping losses.',
  },
  bloat: {
    fraction: 0.8,
    marginMs: 100,
    reason: 'Weir must show bufferbloat that is physically there: at least 80 % of the rise a concurrent ping sees on the deep buffer, and clearly more than on the shallow one.',
  },
  dataCeiling: {
    mb: 1000,
    reason: 'Before a first result the app promises a test uses up to 1 GB (data_estimate UpTo; 600 MB until F-1, F-3 and F-4 were fixed). Counted as IP-layer bytes on the client interface over the whole run.',
  },
  baseline: {
    minMbps: 1500,
    maxServerCoresPerGbps: 0.5,
    maxInflationMs: 1,
    reason: 'Unshaped, iperf3 and Weir must carry 1.5× the fastest shaped link and the lab server must need under half a core per Gbps (the spike measured about a quarter), or the 1 Gbps profiles would judge the lab instead of Weir.',
  },
  unjudged: {
    maxLinks: 1,
    reason: 'Inconclusive and invalid-input verdicts mean the lab could not judge a link, not that Weir is wrong, so they warn. One such link per run is lab noise (the eighth run: one fiber-300-far iperf3 run read 1.016 of the ceiling); two or more mean the lab itself is off, and the gate fails.',
  },
  noise: {
    fractionOfBand: 0.5,
    reason: 'When the truth itself moves by more than half the band between repeats, a pass or fail would be luck. The result is inconclusive and the fix is in the lab.',
  },
  rate: {
    min: 0.93,
    max: 1.015,
    reason: 'Loss-free iperf3 goodput over the shaped payload ceiling. Below 0.93 the shaper or host is not delivering the configured rate; above 1.015 the shaper leaks. Raised from 1.005 after the first three runs: iperf3 read up to 1.1 % over the ceiling on 5–25 Mbps links whose tbf rate is exact, which is iperf3 interval timing, not a leak. The upper bound applies to the median run per direction since the eighth run (fiber-300-far read 1.016 once and 0.999 twice: a leaking shaper leaks every run). The lower bound applies to the best iperf3 run per direction since the seventh run: one gig-60 upload run reached 0.81 while the others reached 0.99, which is TCP on a shallow buffer, not the shaper; such an under-filling run is not used as capacity truth.',
  },
  iperfConsistency: {
    maxPct: 2,
    reason: "iperf3's receiver intervals must agree with its own end total, or that run is not used as truth. Raised from 1 % after the first three runs: a 600 ms buffer upload disagreed by 1.06 %, the queue draining after the sender stops; the speed bands are ten times wider.",
  },
  jitterDelay: {
    medianAbsMs: 1.5,
    medianPct: 10,
    p95Spreads: 4,
    minSpreadMs: 2,
    reason: "netem's paretonormal delay is centred on the configured delay and spreads both ways. On the spike the idle p95 sat 1.9 and 2.5 spreads above the round trip, so 4 spreads bounds it; a p95 − p5 under 2 ms means the jitter is missing.",
  },
  bloatInput: {
    deepFraction: 0.6,
    shallowSlackMs: 10,
    reason: "Under iperf3, cubic's sawtooth keeps the median queue at about three quarters of a deep buffer (spike: 0.75 down, 0.88 up), so 0.6 proves the buffer fills; a shallow buffer may add its own depth plus 10 ms.",
  },
  delay: {
    absMs: 0.5,
    pct: 3,
    reason: 'Idle ping must match the configured netem delay before any number from that link is trusted.',
  },
} as const satisfies Record<string, Threshold>;

export type Thresholds = typeof THRESHOLDS;
