// Pure parsers for tool output. Only numbers leave these functions: never
// hostnames, addresses or raw text.

export const PROBE_TIMEOUT_MS = 2000;

export interface IperfResult {
  /** Receiver-side goodput over the non-omitted intervals. */
  mbps: number;
  /** iperf3's own receiver total, for the cross-check. */
  endMbps: number;
  /** |mbps − endMbps| / endMbps. */
  mismatch: number;
  streams: number;
  seconds: number;
  /** Unix ms of the first counted interval's start, from iperf3's own clock. */
  startMs: number;
  retransmits: number | null;
}

interface IperfInterval {
  sum?: { bytes?: number; seconds?: number; omitted?: boolean; start?: number; end?: number };
}

interface IperfJson {
  start?: { timestamp?: { timesecs?: number }; test_start?: { num_streams?: number; reverse?: number } };
  intervals?: IperfInterval[];
  end?: { sum_received?: { bits_per_second?: number }; sum_sent?: { retransmits?: number } };
  server_output_json?: IperfJson;
  error?: string;
}

/**
 * Goodput from iperf3 -J. With -R the client is the receiver; without it the
 * receiver's intervals come from --get-server-output.
 */
export function parseIperf(text: string, omitSeconds = 2): IperfResult {
  const json = JSON.parse(text) as IperfJson;
  if (json.error) throw new Error(`iperf3: ${json.error.replace(/\d+\.\d+\.\d+\.\d+/g, '<addr>')}`);
  const reverse = Boolean(json.start?.test_start?.reverse);
  const receiver = reverse ? json : json.server_output_json;
  if (!receiver?.intervals?.length) throw new Error('iperf3: no receiver intervals');
  // iperf3 3.16's server does not always flag its omitted intervals, so drop them by time when none are flagged.
  const flagged = receiver.intervals.some((interval) => interval.sum?.omitted);
  // It can also report the omit boundary as one interval whose seconds span two intervals but whose bytes
  // cover one (start equals end); such inconsistent intervals are dropped, and so is the short interval
  // after the sender stops, which only holds the wait for the end of the test.
  const counted = receiver.intervals.filter((interval) => {
    const sum = interval.sum;
    if (!sum) return false;
    const span = (sum.end ?? 0) - (sum.start ?? 0);
    if (Math.abs(span - (sum.seconds ?? 0)) > 0.05) return false;
    if ((sum.seconds ?? 0) < 0.9) return false;
    return flagged ? !sum.omitted : (sum.start ?? 0) >= omitSeconds - 0.001;
  });
  const bytes = counted.reduce((sum, interval) => sum + (interval.sum?.bytes ?? 0), 0);
  const seconds = counted.reduce((sum, interval) => sum + (interval.sum?.seconds ?? 0), 0);
  if (seconds <= 0) throw new Error('iperf3: no counted time');
  const mbps = (bytes * 8) / seconds / 1e6;
  const endMbps = (json.end?.sum_received?.bits_per_second ?? 0) / 1e6;
  const startSecs = json.start?.timestamp?.timesecs ?? 0;
  const firstStart = counted[0]?.sum?.start ?? 0;
  return {
    mbps,
    endMbps,
    mismatch: endMbps > 0 ? Math.abs(mbps - endMbps) / endMbps : 1,
    streams: json.start?.test_start?.num_streams ?? 0,
    seconds,
    startMs: (startSecs + firstStart) * 1000,
    retransmits: json.end?.sum_sent?.retransmits ?? null,
  };
}

/**
 * iperf3 read/write size for a shaped rate. The default 128 KiB reads make a
 * receiver's per-interval bytes jump in 128 KiB steps, which at a few Mbps is
 * several percent of a ten-second test; about a hundredth of a second of data
 * per read keeps that edge error under one percent.
 */
export function iperfBlock(mbps: number): number {
  const bytes = (mbps * 1e6) / 8 / 100;
  return Math.min(131_072, Math.max(2048, Math.round(bytes / 1024) * 1024));
}

export interface PingSample {
  sentMs: number;
  rttMs: number;
}

/**
 * `ping -D -O -i <interval>` text. A reply's send time is its receive time
 * minus its RTT; an unanswered probe counts as the 2000 ms timeout, like
 * Weir's probes. Probes sent in the last 2 s with no reply yet are dropped.
 */
export function parsePing(text: string, intervalMs: number): PingSample[] {
  const replies = new Map<number, PingSample>();
  const missing = new Map<number, number>();
  let lastMs = 0;
  for (const line of text.split('\n')) {
    const stamp = /^\[(\d+\.\d+)\]/.exec(line);
    if (!stamp) continue;
    const at = Number(stamp[1]) * 1000;
    lastMs = Math.max(lastMs, at);
    const reply = /icmp_seq=(\d+).*time=([\d.]+) ms/.exec(line);
    if (reply) {
      const rtt = Number(reply[2]);
      replies.set(Number(reply[1]), { sentMs: at - rtt, rttMs: Math.min(rtt, PROBE_TIMEOUT_MS) });
      continue;
    }
    const lost = /no answer yet for icmp_seq=(\d+)/.exec(line);
    if (lost) {
      const seq = Number(lost[1]);
      if (!missing.has(seq)) missing.set(seq, at - intervalMs);
    }
  }
  const samples = [...replies.values()];
  for (const [seq, sentMs] of missing) {
    if (replies.has(seq) || sentMs > lastMs - PROBE_TIMEOUT_MS) continue;
    samples.push({ sentMs, rttMs: PROBE_TIMEOUT_MS });
  }
  return samples.sort((a, b) => a.sentMs - b.sentMs);
}

export function within(samples: PingSample[], fromMs: number, toMs: number): number[] {
  return samples.filter((sample) => sample.sentMs >= fromMs && sample.sentMs < toMs).map((sample) => sample.rttMs);
}

export function percentile(values: number[], p: number): number | null {
  const sorted = values.filter(Number.isFinite).sort((a, b) => a - b);
  if (!sorted.length) return null;
  const rank = Math.min(1, Math.max(0, p)) * (sorted.length - 1);
  const low = Math.floor(rank);
  const high = Math.ceil(rank);
  return sorted[low]! + (sorted[high]! - sorted[low]!) * (rank - low);
}

export const median = (values: number[]) => percentile(values, 0.5);

/** Mean absolute difference of consecutive values (Cloudflare's and Weir's jitter). */
export function jitter(values: number[]): number | null {
  if (values.length < 2) return null;
  let total = 0;
  for (let index = 1; index < values.length; index++) total += Math.abs(values[index]! - values[index - 1]!);
  return total / (values.length - 1);
}

export interface CpuTimes {
  busy: number;
  total: number;
}

/** Per-CPU times from /proc/stat (the aggregate "cpu" line is skipped). */
export function parseProcStat(text: string): CpuTimes[] {
  const cpus: CpuTimes[] = [];
  for (const line of text.split('\n')) {
    const match = /^cpu\d+\s+(.*)$/.exec(line);
    if (!match) continue;
    const values = match[1]!.trim().split(/\s+/).map(Number);
    const idle = (values[3] ?? 0) + (values[4] ?? 0);
    const total = values.slice(0, 8).reduce((sum, value) => sum + value, 0);
    cpus.push({ busy: total - idle, total });
  }
  return cpus;
}

/** Busiest single CPU between two /proc/stat reads, 0–1. */
export function cpuMax(before: CpuTimes[], after: CpuTimes[]): number {
  let max = 0;
  after.forEach((cpu, index) => {
    const start = before[index];
    if (!start) return;
    const total = cpu.total - start.total;
    if (total > 0) max = Math.max(max, (cpu.busy - start.busy) / total);
  });
  return max;
}

/**
 * Busiest CPU over the busiest `span` consecutive sample intervals. A whole-step average hides a core
 * pinned only while data moves; this does not.
 */
export function cpuPeak(samples: CpuTimes[][], span: number): number {
  if (samples.length < 2) return 0;
  const width = Math.max(1, Math.min(span, samples.length - 1));
  let peak = 0;
  for (let index = 0; index + width < samples.length; index++) peak = Math.max(peak, cpuMax(samples[index]!, samples[index + width]!));
  return peak;
}

/** utime + stime in clock ticks from /proc/<pid>/stat. */
export function parsePidStat(text: string): number {
  const close = text.lastIndexOf(')');
  const fields = text.slice(close + 2).split(' ');
  return Number(fields[11] ?? 0) + Number(fields[12] ?? 0);
}

export interface Wire {
  rxBytes: number;
  rxPackets: number;
  txBytes: number;
  txPackets: number;
}

export interface WireSample extends Wire {
  atMs: number;
}

const ETHERNET = 14;
const IP_TCP = 52;

/** TCP payload in a byte/packet counter delta: minus Ethernet, IPv4 and TCP (with timestamps) headers. */
export function payloadBytes(bytes: number, packets: number): number {
  return bytes - (ETHERNET + IP_TCP) * packets;
}

/** IP-layer bytes (what a provider counts) in a byte/packet counter delta. */
export function ipBytes(bytes: number, packets: number): number {
  return bytes - ETHERNET * packets;
}

/** Linear interpolation of a counter series at `atMs`. */
export function interpolate(samples: WireSample[], atMs: number, key: keyof Wire): number | null {
  if (!samples.length || atMs < samples[0]!.atMs || atMs > samples[samples.length - 1]!.atMs) return null;
  let index = samples.findIndex((sample) => sample.atMs >= atMs);
  if (index <= 0) return samples[Math.max(0, index)]![key];
  const after = samples[index]!;
  const before = samples[index - 1]!;
  const part = (atMs - before.atMs) / (after.atMs - before.atMs || 1);
  return before[key] + (after[key] - before[key]) * part;
}

/** Goodput in Mbps carried by an rx counter between two unix-ms times. */
export function wireMbps(samples: WireSample[], fromMs: number, toMs: number, dir: 'rx' | 'tx' = 'rx'): number | null {
  const bytesKey = dir === 'rx' ? 'rxBytes' : 'txBytes';
  const packetsKey = dir === 'rx' ? 'rxPackets' : 'txPackets';
  const b0 = interpolate(samples, fromMs, bytesKey);
  const b1 = interpolate(samples, toMs, bytesKey);
  const p0 = interpolate(samples, fromMs, packetsKey);
  const p1 = interpolate(samples, toMs, packetsKey);
  if (b0 === null || b1 === null || p0 === null || p1 === null || toMs <= fromMs) return null;
  return (payloadBytes(b1 - b0, p1 - p0) * 8) / ((toMs - fromMs) * 1000);
}
