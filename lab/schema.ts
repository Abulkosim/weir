// Results JSON, schema 1. Numbers only: no addresses, hostnames, SSIDs or places.

import type { Shape } from './net.ts';
import type { Step } from './profiles.ts';

export interface Speed {
  mbps: number;
  p10: number;
  p90: number;
  varied: boolean;
  capped: boolean;
  rampedSlowly: boolean;
  rough: boolean;
  loadedMs: number | null;
}

/** The engine's Report as the lab example serializes it. */
export interface WeirReport {
  durationMs: number;
  latency: { medianMs: number; jitterMs: number | null; answered: number; sent: number } | null;
  download: Speed;
  upload: Speed | null;
  uploadError: string | null;
  bytesUsed: number;
}

export interface Window {
  phaseStartUnixMs: number;
  steadyStartMs: number | null;
  endMs: number;
  startUnixMs: number | null;
  endUnixMs: number;
  rebuiltMbps: number | null;
  phaseBytes: number;
}

export interface Failed {
  error: string;
}

export interface Truth {
  downMbps: number;
  upMbps: number;
  idleMs: number | null;
  jitterMs: number | null;
  loadedDownMs: number | null;
  loadedUpMs: number | null;
  streams: { down: number; up: number };
  /** |intervals − end sum| / end sum per direction; over 1 % the judge is invalid. */
  mismatch: { down: number; up: number };
  cpuMax: number;
  saturated: boolean;
}

export interface WeirWire {
  downMbps: number | null;
  upMbps: number | null;
  idleMs: number | null;
  loadedDownMs: number | null;
  loadedUpMs: number | null;
  /** IP-layer bytes both ways on the client interface over the whole run. */
  ipBytes: number | null;
  /** TCP payload both ways on the client interface over the whole run. */
  payloadBytes: number | null;
  pingSent: number;
  /** Server wire samples around the upload window: ms before its start, ms after its end, count. */
  serverSpan: [number, number, number] | null;
}

export interface UploadRequest {
  startMs: number;
  bodyEndMs: number;
  bytes: number;
}

export interface Weir {
  report: WeirReport | null;
  error: string | null;
  wallMs: number;
  windows: { download: Window | null; upload: Window | null };
  wire: WeirWire;
  serverBytes: { down: number; up: number; unknown: number };
  far: boolean | null;
  cpuMax: number;
  saturated: boolean;
  uploadRequests?: UploadRequest[];
  trace?: unknown[];
}

export interface Twin {
  down: number | null;
  up: number | null;
  idle: number | null;
  jitter: number | null;
  downLoaded: number | null;
  upLoaded: number | null;
  durationMs: number | null;
  errors: string[];
  blocked: number;
  cpuMax: number;
  saturated: boolean;
}

export interface Repeat {
  order: Step[];
  wallMs: number;
  truth: Truth | Failed | null;
  weir: Weir | Failed;
  twin: Twin | Failed | null;
}

export interface Check {
  ok: boolean;
  detail: string;
}

export interface Validity {
  delay: Check & { medianMs: number | null; p5: number | null; p95: number | null };
  rate: (Check & { ratios: number[] }) | null;
  loss: (Check & { down: number | null; up: number | null }) | null;
  drops: Check & { softnet: number; txDropped: number };
  bloat: (Check & { loadedMs: number | null }) | null;
  reorder: (Check & { events: number }) | null;
  ok: boolean;
  reasons: string[];
}

export interface ProfileResult {
  id: string;
  shape: Shape & { serverMs: number; uploadTimingIncludesBody: boolean };
  shard: number;
  planned: number;
  offloads: 'off' | 'gso-on';
  validity: Validity;
  repeats: Repeat[];
}

export interface Baseline {
  iperfDown: number | null;
  iperfUp: number | null;
  weirDown: number | null;
  weirUp: number | null;
  weirError: string | null;
  twinDown: number | null;
  twinUp: number | null;
  serverCpuCores: number | null;
  serverGbps: number | null;
  softnetDrops: number;
  weirLoadedInflationMs: number | null;
}

export interface ShardResult {
  shard: number;
  wallMs: number;
  cpuModel: string;
  kernel: string;
  baseline: Baseline;
}

export interface Runner {
  kernel: string;
  cpuModel: string;
  cpus: number;
  iperf3: string;
  chromium: string;
  speedtestLib: string;
  tcpCc: string;
  sysctls: Record<string, string>;
  offloads: string;
  jitterMode: string;
  netdevMaxBacklog: number;
  weirProfile: string;
}

export interface Results {
  schema: 1;
  commit: string;
  weirVersion: string;
  runUrl: string;
  startedAt: string;
  runner: Runner;
  shards: ShardResult[];
  profiles: ProfileResult[];
}

/** One shard's output before report.ts merges them. */
export interface ShardFile {
  schema: 1;
  shard: number;
  runner: Runner;
  startedAt: string;
  result: ShardResult;
  profiles: ProfileResult[];
}

export const failed = (value: unknown): value is Failed =>
  typeof value === 'object' && value !== null && 'error' in value && typeof (value as Failed).error === 'string' && !('report' in value);

/** Drops per-tick traces and request logs, for the committed docs/accuracy.json. */
export function slim(results: Results): Results {
  return {
    ...results,
    profiles: results.profiles.map((entry) => ({
      ...entry,
      repeats: entry.repeats.map((repeat) => {
        if (failed(repeat.weir)) return repeat;
        const { trace: _trace, uploadRequests: _requests, ...weir } = repeat.weir;
        return { ...repeat, weir };
      }),
    })),
  };
}
