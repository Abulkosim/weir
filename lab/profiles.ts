// The lab's shaped links. Data only: lab/run.ts runs them, lab/analyze.ts judges them.

import type { Shape } from './net.ts';

export type Step = 'truth' | 'weir' | 'twin';

export interface Profile extends Shape {
  id: string;
  /** What a reader should picture. */
  label: string;
  /** Added before every response head, like an edge doing work. */
  serverMs: number;
  /** The lab server's upload Server-Timing covers the body transfer too (Q-real). */
  uploadTimingIncludesBody: boolean;
  repeats: number;
  twinRepeats: number;
  shard: number;
  /** Shallow-vs-deep buffer pair at the same rate. */
  pair?: { role: 'bloat' | 'shallow'; with: string };
  /** Numbers shown but never gated. */
  reportOnly: boolean;
  /** Runs truth steps; false runs Weir alone. */
  truth: boolean;
}

export const SHARDS = 5;
export const FAR_MS = 100;

const base = {
  serverMs: 0,
  uploadTimingIncludesBody: false,
  lossPct: 0,
  repeats: 3,
  twinRepeats: 3,
  reportOnly: false,
  truth: true,
};

export const PROFILES: Profile[] = [
  { ...base, id: 'slow-5', label: 'Slow DSL, 5/1 Mbps', down: 5, up: 1, rttMs: 40, bufferMs: 200, shard: 1 },
  { ...base, id: 'gig-5', label: 'Gigabit fibre, near', down: 1000, up: 1000, rttMs: 5, bufferMs: 10, shard: 1 },
  { ...base, id: 'gig-60', label: 'Gigabit, 60 ms away', down: 1000, up: 500, rttMs: 60, bufferMs: 20, shard: 1 },
  { ...base, id: 'fiber-300', label: 'Fibre 300/300', down: 300, up: 300, rttMs: 10, bufferMs: 30, shard: 1 },
  { ...base, id: 'dsl-25-bloat', label: 'DSL 25/5, deep buffer', down: 25, up: 5, rttMs: 30, bufferMs: 600, shard: 2, pair: { role: 'bloat', with: 'dsl-25-shallow' } },
  { ...base, id: 'dsl-25-shallow', label: 'DSL 25/5, shallow buffer', down: 25, up: 5, rttMs: 30, bufferMs: 20, shard: 2, pair: { role: 'shallow', with: 'dsl-25-bloat' } },
  { ...base, id: 'cable-100-10', label: 'Cable 100/10', down: 100, up: 10, rttMs: 20, bufferMs: 100, shard: 2 },
  { ...base, id: 'cable-100-10-cf', label: 'Cable 100/10, 30 ms server time', down: 100, up: 10, rttMs: 20, bufferMs: 100, serverMs: 30, shard: 2 },
  {
    ...base, id: 'qreal-up', label: 'Cable 100/10, upload server time includes the body', down: 100, up: 10, rttMs: 20, bufferMs: 100,
    serverMs: 30, uploadTimingIncludesBody: true, twinRepeats: 0, reportOnly: true, truth: false, shard: 2,
  },
  { ...base, id: 'geo-250', label: 'Satellite-like, 250 ms', down: 25, up: 3, rttMs: 250, bufferMs: 300, shard: 3 },
  { ...base, id: 'fiber-300-far', label: 'Fibre 300/100, 120 ms away', down: 300, up: 100, rttMs: 120, bufferMs: 50, shard: 3 },
  { ...base, id: 'mobile', label: 'Mobile 30/8, jittery, 0.5% loss', down: 30, up: 8, rttMs: 50, lossPct: 0.5, bufferMs: 300, jitter: { ms: 6 }, serverMs: 25, repeats: 5, shard: 3 },
  { ...base, id: 'lossy-0.1', label: 'Fibre 300/50, 0.1% loss', down: 300, up: 50, rttMs: 20, lossPct: 0.1, bufferMs: 30, repeats: 5, shard: 4 },
  { ...base, id: 'lossy-1', label: 'Cable 100/20, 1% loss', down: 100, up: 20, rttMs: 40, lossPct: 1, bufferMs: 50, repeats: 5, shard: 4 },
  { ...base, id: 'lossy-2', label: 'DSL 25/5, 2% loss, 80 ms', down: 25, up: 5, rttMs: 80, lossPct: 2, bufferMs: 100, repeats: 5, shard: 5 },
  { ...base, id: 'wifi-jitter', label: 'Busy Wi-Fi 200/50, jittery', down: 200, up: 50, rttMs: 15, lossPct: 0.2, bufferMs: 50, jitter: { ms: 3 }, repeats: 5, shard: 5 },
];

export function profile(id: string): Profile {
  const found = PROFILES.find((entry) => entry.id === id);
  if (!found) throw new Error(`unknown profile ${id}`);
  return found;
}

/** Weir adds two streams each way when its idle latency is over 100 ms. */
export function expectedFar(entry: Pick<Profile, 'rttMs'>): boolean {
  return entry.rttMs > FAR_MS;
}

export function streams(far: boolean): { down: number; up: number } {
  return far ? { down: 8, up: 6 } : { down: 6, up: 4 };
}

/** Steps for one repeat, rotated so no tool always runs first: (T,W,C), (W,C,T), (C,T,W), … */
export function order(entry: Profile, repeat: number): Step[] {
  const all: Step[] = ['truth', 'weir', 'twin'];
  const rotated = [...all.slice(repeat % 3), ...all.slice(0, repeat % 3)];
  return rotated.filter((step) => (step === 'truth' ? entry.truth : step === 'twin' ? repeat < entry.twinRepeats : true));
}

/** Theoretical TCP goodput ceiling of a shaped rate: payload per Ethernet frame. */
export function ceilingMbps(mbps: number): number {
  return (mbps * 1448) / 1514;
}

export function forShard(shard: number): Profile[] {
  return PROFILES.filter((entry) => entry.shard === shard);
}

export interface SelectionInput {
  /** Comma-separated link ids; empty means every link. */
  profiles?: string;
  /** Repeats per link; empty keeps each link's own. */
  repeats?: string;
  /** Comma-separated shard numbers; empty means every shard. */
  shards?: string;
}

const list = (text: string | undefined) => (text ?? '').split(',').map((item) => item.trim()).filter(Boolean);

/** The links a run covers, with any repeat override applied. Throws on an unknown id or a bad number. */
export function selection(input: SelectionInput = {}): Profile[] {
  const ids = list(input.profiles);
  const unknown = ids.filter((id) => !PROFILES.some((entry) => entry.id === id));
  if (unknown.length) throw new Error(`unknown link ${unknown.join(', ')}; known links: ${PROFILES.map((entry) => entry.id).join(', ')}`);
  const shards = list(input.shards).map((text) => {
    if (!/^\d+$/.test(text) || Number(text) < 1 || Number(text) > SHARDS) throw new Error(`shard must be 1 to ${SHARDS}, not "${text}"`);
    return Number(text);
  });
  const repeatsText = (input.repeats ?? '').trim();
  if (repeatsText && !/^[1-9]\d*$/.test(repeatsText)) throw new Error(`repeats must be a positive whole number, not "${repeatsText}"`);
  const repeats = repeatsText ? Number(repeatsText) : null;
  return PROFILES
    .filter((entry) => (!ids.length || ids.includes(entry.id)) && (!shards.length || shards.includes(entry.shard)))
    .map((entry) => (repeats === null ? entry : { ...entry, repeats, twinRepeats: Math.min(entry.twinRepeats, repeats) }));
}

/** What the lab does not cover, rendered into docs/accuracy.md. */
export const LEFT_OUT = [
  'Bursty (Gilbert-Elliott) loss; reordering, duplication and corruption.',
  'AQM bottlenecks (fq_codel, cake): every queue here is a tail-drop FIFO.',
  "BBR or any server congestion control other than cubic (Cloudflare's own is unknown).",
  'TLS (production uses rustls; TLS CPU at 1 Gbps is not covered), HTTP/2 and HTTP/3.',
  'IPv6, MTU and path-MTU problems, proxies and DNS.',
  'Cross-traffic from other devices, rate changes mid-test (Wi-Fi rate adaptation) and network changes mid-test.',
  "Cloudflare's anycast edge, its throttles and Referer gates.",
  'macOS and Windows TCP stacks and schedulers: the lab runs Linux only.',
  'Loss over 2 %, RTT over 250 ms and queues over about 1.5 s (Weir clamps probes at 2000 ms).',
  'Dependency drift between engine changes: the lab runs on engine changes and weekly, not on every lockfile bump.',
];
