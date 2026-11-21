// Accuracy lab orchestrator. Runs one shard of the profile matrix on a Linux
// host with sudo and writes numbers only (docs/accuracy.md explains the setup).
//
// node lab/run.ts --shard 2/5 --weir <bin> --server <bin> --twin <dir> --out lab-out/shard-2.json
//   [--profiles a,b] [--repeats N] [--jitter pareto|slot]

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { asRoot, clientUser, must, run, runAll, sleep, started, type Background } from './exec.ts';
import {
  ADDR, CLIENT_SYSCTLS, IFACE, NS, PORT, inNs, offloads, parseNstat, parseQdiscStats, parseSoftnet, qdiscStats,
  reorderDelta, shape, teardown, topologyUp, unshape, type JitterMode,
} from './net.ts';
import {
  cpuMax, cpuPeak, iperfBlock, ipBytes, jitter, median, parseIperf, PROBE_TIMEOUT_MS, parsePidStat, parsePing, parseProcStat, payloadBytes, within,
  wireMbps, type CpuTimes, type PingSample, type WireSample,
} from './parse.ts';
import { expectedFar, order, selection, SHARDS, streams, type Profile, type Step } from './profiles.ts';
import {
  slim, type Baseline, type Failed, type ProfileResult, type Repeat, type Results, type Runner, type ShardFile, type Truth, type Twin, type Weir,
  type WeirReport, type Window,
} from './schema.ts';
import { retestable, validity, type Finding } from './analyze.ts';

const SATURATED = 0.95;
const CPU_SAMPLE_MS = 250;
/** Four samples: the busiest second. */
const CPU_SPAN = 4;
const PING_INTERVAL_S = 0.05;

const flags = new Map<string, string>();
for (let index = 2; index < process.argv.length; index += 2) flags.set(process.argv[index]!.replace(/^--/, ''), process.argv[index + 1] ?? '');
const [shardText, totalText] = (flags.get('shard') ?? '1/5').split('/');
const SHARD = Number(shardText);
const weirBin = flags.get('weir') ?? 'lab-bin/lab';
const serverBin = flags.get('server') ?? 'lab-bin/weir-lab-server';
const twinDir = flags.get('twin') ?? 'lab-bin/twin';
const out = flags.get('out') ?? `lab-out/shard-${SHARD}.json`;
const jitterMode = (flags.get('jitter') || 'pareto') as JitterMode;
const BASE = `http://${ADDR.server}:${PORT}`;

const round = (value: number | null | undefined, digits = 3) =>
  value === null || value === undefined || !Number.isFinite(value) ? null : Number(value.toFixed(digits));
const log = (line: string) => console.log(`[lab ${new Date().toISOString().slice(11, 19)}] ${line}`);

function cpu(): CpuTimes[] {
  return parseProcStat(readFileSync('/proc/stat', 'utf8'));
}

/** Samples /proc/stat through a step; `stop` returns the whole-step busiest CPU and the busiest second. */
function cpuSampler(): { stop: () => { whole: number; peak: number } } {
  const samples: CpuTimes[][] = [cpu()];
  const timer = setInterval(() => samples.push(cpu()), CPU_SAMPLE_MS);
  return {
    stop: () => {
      clearInterval(timer);
      samples.push(cpu());
      return { whole: cpuMax(samples[0]!, samples[samples.length - 1]!), peak: cpuPeak(samples, CPU_SPAN) };
    },
  };
}

function softnetDropped(): number {
  return parseSoftnet(readFileSync('/proc/net/softnet_stat', 'utf8')).dropped;
}

async function serverControl(path: string, method: 'GET' | 'POST' = 'GET'): Promise<string> {
  return must(asRoot(inNs(NS.server, 'curl', '-s', '-f', '-X', method, `${BASE}${path}`)), 15_000);
}

async function qdisc(dev: string) {
  return parseQdiscStats(await must(asRoot(qdiscStats(dev))));
}

async function drain(): Promise<void> {
  const deadline = Date.now() + 10_000;
  while (Date.now() < deadline) {
    const stats = [...(await qdisc(IFACE.routerToClient)), ...(await qdisc(IFACE.routerToServer))];
    if (stats.every((entry) => entry.backlog === 0 && entry.qlen === 0)) break;
    await sleep(200);
  }
  await sleep(1000);
}

async function txDropped(): Promise<number> {
  const read = async (ns: string, dev: string) =>
    Number((await must(asRoot(inNs(ns, 'cat', `/sys/class/net/${dev}/statistics/tx_dropped`)))).trim());
  return (await read(NS.client, IFACE.client)) + (await read(NS.server, IFACE.server));
}

async function nstat(ns: string) {
  return parseNstat(await must(asRoot(inNs(ns, 'nstat', '-asz'))));
}

function pinger(intervalS: number): Background {
  return started(asRoot(inNs(NS.client, 'ping', '-D', '-O', '-n', '-i', String(intervalS), ADDR.server)));
}

async function idlePing(count: number): Promise<number[]> {
  const done = await run(asRoot(inNs(NS.client, 'ping', '-D', '-O', '-n', '-c', String(count), '-i', '0.05', '-W', '2', ADDR.server)), 20_000);
  return parsePing(done.stdout, 50).slice(1).map((sample) => sample.rttMs);
}

/** The lab server inside the server netns, never a stale one elsewhere on the host. */
async function serverPid(): Promise<number | null> {
  const pids = (await run(asRoot(['ip', 'netns', 'pids', NS.server]))).stdout.trim().split('\n').map(Number).filter((pid) => pid > 0);
  for (const pid of pids) {
    try {
      if (readFileSync(`/proc/${pid}/comm`, 'utf8').trim() === 'weir-lab-server') return pid;
    } catch {
      // gone
    }
  }
  return null;
}

type Counters = NonNullable<TraceRow['wire']>;

async function clientCounters(): Promise<Counters | null> {
  const names = ['rx_bytes', 'rx_packets', 'tx_bytes', 'tx_packets'];
  const done = await run(asRoot(inNs(NS.client, 'cat', ...names.map((name) => `/sys/class/net/${IFACE.client}/statistics/${name}`))));
  const values = done.stdout.trim().split('\n').map(Number);
  if (done.code !== 0 || values.length !== 4 || values.some((value) => !Number.isFinite(value))) return null;
  const [rxBytes, rxPackets, txBytes, txPackets] = values as [number, number, number, number];
  return { rxBytes, rxPackets, txBytes, txPackets };
}

/**
 * Client counters once they stop moving after Weir exits: upload bytes still in the socket send buffers
 * cross the interface after the engine returns, and they are data the test used.
 */
async function settledCounters(): Promise<Counters | null> {
  const deadline = Date.now() + 5000;
  let last = await clientCounters();
  let stableSince = Date.now();
  while (last && Date.now() < deadline) {
    await sleep(100);
    const now = await clientCounters();
    if (!now) return last;
    if (now.rxBytes !== last.rxBytes || now.txBytes !== last.txBytes) stableSince = Date.now();
    else if (Date.now() - stableSince >= 500) return now;
    last = now;
  }
  return last;
}

function pidTicks(pid: number | null): number {
  if (!pid) return 0;
  try {
    return parsePidStat(readFileSync(`/proc/${pid}/stat`, 'utf8'));
  } catch {
    return 0;
  }
}

async function iperf(dir: 'down' | 'up', parallel: number, seconds: number, mbps = 10_000): Promise<{ mbps: number; mismatch: number; loadedMs: number | null } | Failed> {
  const ping = pinger(PING_INTERVAL_S);
  const begin = Date.now();
  const argv = [
    'iperf3', '-c', ADDR.server, '-P', String(parallel), '-t', String(seconds), '-O', '2', '-l', String(iperfBlock(mbps)), '-J',
    dir === 'down' ? '-R' : '--get-server-output',
  ];
  const done = await run(clientUser(argv), (seconds + 20) * 1000);
  const pings = parsePing((await ping.stop()).stdout, PING_INTERVAL_S * 1000);
  if (done.code !== 0) return { error: done.timedOut ? 'timeout' : `iperf3 exit ${done.code}` };
  try {
    const parsed = parseIperf(done.stdout);
    return { mbps: parsed.mbps, mismatch: parsed.mismatch, loadedMs: median(within(pings, begin + 3000, begin + 10_000)) };
  } catch (error) {
    return { error: String(error).slice(0, 200) };
  }
}

/** Below 10 Mbps one receive burst after a tail drop is a few percent of a ten-second test, so slow links run longer. */
function truthSeconds(mbps: number): number {
  return mbps < 10 ? 20 : 10;
}

async function truthStep(entry: Profile): Promise<Truth | Failed> {
  const sampler = cpuSampler();
  const idle = await idlePing(21);
  const count = streams(expectedFar(entry));
  const down = await iperf('down', count.down, truthSeconds(entry.down), entry.down);
  if ('error' in down) {
    sampler.stop();
    return down;
  }
  await drain();
  const up = await iperf('up', count.up, truthSeconds(entry.up), entry.up);
  const load = sampler.stop();
  if ('error' in up) return up;
  return {
    downMbps: round(down.mbps)!,
    upMbps: round(up.mbps)!,
    idleMs: round(median(idle)),
    jitterMs: round(jitter(idle)),
    loadedDownMs: round(down.loadedMs),
    loadedUpMs: round(up.loadedMs),
    streams: count,
    mismatch: { down: round(down.mismatch, 5)!, up: round(up.mismatch, 5)! },
    cpuMax: round(load.whole)!,
    cpuPeak: round(load.peak)!,
    saturated: load.peak > SATURATED,
  };
}

interface ServerWire {
  columns: string[];
  samples: number[][];
}

function serverSamples(text: string): WireSample[] {
  const parsed = JSON.parse(text) as ServerWire;
  return parsed.samples.map(([atMs, rxBytes, rxPackets, txBytes, txPackets]) => ({
    atMs: atMs!, rxBytes: rxBytes!, rxPackets: rxPackets!, txBytes: txBytes!, txPackets: txPackets!,
  }));
}

interface TraceRow {
  unixMs: number;
  phase: string;
  wire: { rxBytes: number; rxPackets: number; txBytes: number; txPackets: number } | null;
}

interface WeirOutput {
  report: WeirReport | null;
  error: string | null;
  wallMs: number;
  windows: { download: Window | null; upload: Window | null };
  wireStart: TraceRow['wire'];
  wireEnd: TraceRow['wire'];
  trace: TraceRow[];
}

function windowPing(pings: PingSample[], window: Window | null): number | null {
  if (!window || window.startUnixMs === null) return null;
  const end = window.phaseEndMs === undefined ? window.endUnixMs : window.phaseStartUnixMs + window.phaseEndMs;
  return round(median(within(pings, window.startUnixMs, end)));
}

const DSACK_SENT = ['TcpExtTCPDSACKOldSent', 'TcpExtTCPDSACKOfoSent'] as const;

async function dsackSent(ns: string): Promise<number> {
  const counters = await nstat(ns);
  return DSACK_SENT.reduce((sum, name) => sum + (counters[name] ?? 0), 0);
}

async function weirStep(extra: string[] = []): Promise<Weir | Failed> {
  const dupBefore = { down: await dsackSent(NS.client), up: await dsackSent(NS.server) };
  const sampler = cpuSampler();
  const ping = pinger(PING_INTERVAL_S);
  await sleep(300);
  const done = await run(clientUser([weirBin, '--base', BASE, '--wire-iface', IFACE.client, ...extra]), 60_000);
  const pingDone = await ping.stop();
  const load = sampler.stop();
  const settled = await settledCounters();
  if (done.code !== 0) return { error: done.timedOut ? 'timeout' : `exit ${done.code}` };
  let output: WeirOutput;
  try {
    output = JSON.parse(done.stdout.trim().split('\n').pop() ?? '') as WeirOutput;
  } catch {
    return { error: 'unreadable output' };
  }
  const pings = parsePing(pingDone.stdout, PING_INTERVAL_S * 1000);
  const duplicates = { down: (await dsackSent(NS.client)) - dupBefore.down, up: (await dsackSent(NS.server)) - dupBefore.up };
  await sleep(200);
  const client: WireSample[] = output.trace.filter((row) => row.wire).map((row) => ({ atMs: row.unixMs, ...row.wire! }));
  const server = serverSamples(await serverControl('/lab/wire'));
  const stats = JSON.parse(await serverControl('/lab/stats')) as { down_bytes: number; up_bytes: number; unknown: number };
  const requests = (JSON.parse(await serverControl('/lab/requests')) as Array<{ kind: string; unix_ms_start: number; unix_ms_body_end: number; bytes: number }>)
    .filter((entry) => entry.kind === 'up')
    .map((entry) => ({ startMs: entry.unix_ms_start, bodyEndMs: entry.unix_ms_body_end, bytes: entry.bytes }));
  const { windows } = output;
  const latencyRows = output.trace.filter((row) => row.phase === 'latency');
  const idleMs = latencyRows.length
    ? median(within(pings, latencyRows[0]!.unixMs - 1000, latencyRows[latencyRows.length - 1]!.unixMs))
    : null;
  const startWire = output.wireStart;
  const returned = output.wireEnd;
  const endWire = settled ?? returned;
  const tail = returned && endWire
    ? ipBytes(endWire.rxBytes - returned.rxBytes, endWire.rxPackets - returned.rxPackets)
      + ipBytes(endWire.txBytes - returned.txBytes, endWire.txPackets - returned.txPackets)
    : null;
  const span = startWire && endWire
    ? {
        ip: ipBytes(endWire.rxBytes - startWire.rxBytes, endWire.rxPackets - startWire.rxPackets)
          + ipBytes(endWire.txBytes - startWire.txBytes, endWire.txPackets - startWire.txPackets),
        payload: payloadBytes(endWire.rxBytes - startWire.rxBytes, endWire.rxPackets - startWire.rxPackets)
          + payloadBytes(endWire.txBytes - startWire.txBytes, endWire.txPackets - startWire.txPackets),
      }
    : null;
  const downWindow = windows.download;
  const upWindow = windows.upload;
  const latency = output.report?.latency?.medianMs;
  return {
    report: output.report,
    error: output.error,
    wallMs: output.wallMs,
    windows,
    wire: {
      downMbps: downWindow?.startUnixMs ? round(wireMbps(client, downWindow.startUnixMs, downWindow.endUnixMs, 'rx')) : null,
      upMbps: upWindow?.startUnixMs ? round(wireMbps(server, upWindow.startUnixMs, upWindow.endUnixMs, 'rx')) : null,
      idleMs: round(idleMs),
      loadedDownMs: windowPing(pings, downWindow),
      loadedUpMs: windowPing(pings, upWindow),
      ipBytes: span ? span.ip : null,
      payloadBytes: span ? span.payload : null,
      tailIpBytes: tail,
      pingSent: pings.length,
      serverSpan: server.length && upWindow?.startUnixMs
        ? [round(upWindow.startUnixMs - server[0]!.atMs, 0)!, round(server[server.length - 1]!.atMs - upWindow.endUnixMs, 0)!, server.length]
        : null,
    },
    serverBytes: { down: stats.down_bytes, up: stats.up_bytes, unknown: stats.unknown },
    far: typeof latency === 'number' ? latency > 100 : null,
    duplicates,
    cpuMax: round(load.whole)!,
    cpuPeak: round(load.peak)!,
    saturated: load.peak > SATURATED,
    uploadRequests: requests,
    trace: output.trace,
  };
}

async function twinStep(): Promise<Twin | Failed> {
  const before = cpu();
  const done = await run(clientUser(['node', 'lab/twin.ts', '--url', `${BASE}/twin/index.html`]), 175_000);
  const busiest = cpuMax(before, cpu());
  const line = done.stdout.trim().split('\n').pop() ?? '';
  try {
    const parsed = JSON.parse(line) as Partial<Twin> & { errors?: string[]; blocked?: number };
    return {
      down: round(parsed.down ?? null),
      up: round(parsed.up ?? null),
      idle: round(parsed.idle ?? null),
      jitter: round(parsed.jitter ?? null),
      downLoaded: round(parsed.downLoaded ?? null),
      upLoaded: round(parsed.upLoaded ?? null),
      durationMs: round(parsed.durationMs ?? null, 0),
      errors: parsed.errors ?? [],
      blocked: parsed.blocked ?? 0,
      cpuMax: round(busiest)!,
      saturated: busiest > SATURATED,
    };
  } catch {
    return { error: done.timedOut ? 'timeout' : `exit ${done.code}` };
  }
}

async function step<T>(name: Step, body: () => Promise<T | Failed>): Promise<T | Failed> {
  await drain();
  await serverControl('/lab/reset', 'POST');
  let result = await body();
  if (typeof result === 'object' && result && 'saturated' in result && (result as { saturated: boolean }).saturated) {
    log(`  ${name}: a CPU was over ${SATURATED * 100} % busy; retrying once`);
    await drain();
    await serverControl('/lab/reset', 'POST');
    result = await body();
  }
  return result;
}

async function setShape(entry: Profile | null): Promise<void> {
  await runAll(unshape(), true);
  if (entry) await runAll(shape(entry, jitterMode));
  await serverControl(`/lab/config?server_ms=${entry?.serverMs ?? 0}&up_includes_body=${entry?.uploadTimingIncludesBody ? 1 : 0}`, 'POST');
  await sleep(500);
}

function summarize(entry: Profile, value: Repeat): string {
  const weir = 'report' in value.weir ? value.weir : null;
  const truth = value.truth && !('error' in value.truth) ? value.truth : null;
  const twin = value.twin && !('error' in value.twin) ? value.twin : null;
  return [
    `${entry.id}`,
    truth ? `truth ${truth.downMbps}/${truth.upMbps} Mbps idle ${truth.idleMs} ms` : `truth ${value.truth ? (value.truth as Failed).error : '-'}`,
    weir?.report ? `weir ${round(weir.report.download.mbps, 1)}/${round(weir.report.upload?.mbps, 1)} Mbps idle ${round(weir.report.latency?.medianMs, 1)} ms wire ${weir.wire.downMbps}/${weir.wire.upMbps} upErr ${weir.report.uploadError}` : `weir ${'error' in value.weir ? value.weir.error : '?'}`,
    twin ? `twin ${round(twin.down, 1)}/${round(twin.up, 1)} errors ${twin.errors.length} blocked ${twin.blocked}` : '',
  ].join(' · ');
}

async function runProfile(entry: Profile): Promise<ProfileResult> {
  log(`profile ${entry.id}`);
  await setShape(entry);
  const softnetBefore = softnetDropped();
  const txBefore = await txDropped();
  const nstatBefore = { c: await nstat(NS.client), s: await nstat(NS.server) };

  await drain();
  const delaySamples = (await idlePing(81)).filter((rtt) => rtt < PROBE_TIMEOUT_MS);
  const repeats: Repeat[] = [];
  const count = entry.repeats;
  for (let index = 0; index < count; index++) {
    const begin = Date.now();
    const steps = order(entry, index);
    const repeat: Repeat = { order: steps, wallMs: 0, truth: null, weir: { error: 'not run' }, twin: null };
    for (const name of steps) {
      if (name === 'truth') repeat.truth = await step('truth', () => truthStep(entry));
      if (name === 'weir') repeat.weir = await step('weir', () => weirStep());
      if (name === 'twin') repeat.twin = await step('twin', () => twinStep());
    }
    repeat.wallMs = Date.now() - begin;
    repeats.push(repeat);
    log(`  ${index + 1}/${count} ${summarize(entry, repeat)}`);
  }

  // netem's drop counter also counts its tbf child's tail drops; only the difference is netem's own loss.
  const lossOf = async (dev: string) => {
    const stats = await qdisc(dev);
    const netemStats = stats.find((entry) => entry.kind === 'netem');
    const tbfStats = stats.find((entry) => entry.kind === 'tbf');
    if (!netemStats || netemStats.packets + netemStats.drops === 0) return null;
    return (netemStats.drops - (tbfStats?.drops ?? 0)) / (netemStats.packets + netemStats.drops);
  };
  const loss = { down: await lossOf(IFACE.routerToClient), up: await lossOf(IFACE.routerToServer) };
  const reorder = reorderDelta(nstatBefore.c, await nstat(NS.client)) + reorderDelta(nstatBefore.s, await nstat(NS.server));
  const softnet = softnetDropped() - softnetBefore;
  const tx = (await txDropped()) - txBefore;
  const truths = repeats.map((value) => value.truth).filter((value): value is Truth => !!value && !('error' in value));
  return {
    id: entry.id,
    shape: {
      down: entry.down, up: entry.up, rttMs: entry.rttMs, lossPct: entry.lossPct, bufferMs: entry.bufferMs,
      ...(entry.jitter ? { jitter: entry.jitter } : {}), serverMs: entry.serverMs, uploadTimingIncludesBody: entry.uploadTimingIncludesBody,
    },
    shard: entry.shard,
    planned: count,
    offloads: 'off',
    validity: validity(entry, delaySamples, truths, loss, softnet, tx, reorder),
    repeats,
  };
}

async function baseline(): Promise<Baseline> {
  log('baseline (unshaped)');
  await setShape(null);
  const softnetBefore = softnetDropped();
  await drain();
  const down = await iperf('down', 8, 8);
  await drain();
  const up = await iperf('up', 8, 8);
  await drain();
  await serverControl('/lab/reset', 'POST');
  const pid = await serverPid();
  const ticks = pidTicks(pid);
  const begin = Date.now();
  const weir = await weirStep(['--baseline']);
  const seconds = (Date.now() - begin) / 1000;
  const cores = (pidTicks(pid) - ticks) / 100 / seconds;
  const moved = 'serverBytes' in weir ? weir.serverBytes.down + weir.serverBytes.up : 0;
  const twin = await step('twin', () => twinStep());
  const report = 'report' in weir ? weir.report : null;
  const inflation = report && 'wire' in weir && report.download.loadedMs !== null && weir.wire.loadedDownMs !== null
    ? report.download.loadedMs - weir.wire.loadedDownMs
    : null;
  const result: Baseline = {
    iperfDown: 'error' in down ? null : round(down.mbps),
    iperfUp: 'error' in up ? null : round(up.mbps),
    weirDown: round(report?.download.mbps),
    weirUp: round(report?.upload?.mbps),
    weirError: 'report' in weir ? (weir.error ?? weir.report?.uploadError ?? null) : weir.error,
    twinDown: 'errors' in twin ? twin.down : null,
    twinUp: 'errors' in twin ? twin.up : null,
    serverCpuCores: round(cores),
    serverGbps: round((moved * 8) / seconds / 1e9),
    softnetDrops: softnetDropped() - softnetBefore,
    weirLoadedInflationMs: round(inflation),
  };
  log(`  ${JSON.stringify(result)}`);
  return result;
}

async function runnerInfo(): Promise<Runner> {
  const kernel = (await must(['uname', '-r'])).trim();
  const cpuModel = /model name\s*:\s*(.*)/.exec(readFileSync('/proc/cpuinfo', 'utf8'))?.[1]?.trim() ?? 'unknown';
  const iperf3 = /iperf (\S+)/.exec((await run(['iperf3', '--version'])).stdout)?.[1] ?? 'missing';
  const chromium = (await run(clientUser(['node', '-e', "import('@playwright/test').then(({chromium})=>chromium.launch()).then(async b=>{console.log(b.version());await b.close()})"]), 60_000)).stdout.trim();
  const lib = JSON.parse(readFileSync('node_modules/@cloudflare/speedtest/package.json', 'utf8')) as { version: string };
  const sysctls: Record<string, string> = {};
  for (const [key] of CLIENT_SYSCTLS) sysctls[key] = (await must(asRoot(inNs(NS.client, 'sysctl', '-n', key)))).trim();
  const tcpCc = sysctls['net.ipv4.tcp_congestion_control'] ?? '';
  return {
    kernel,
    cpuModel,
    cpus: cpu().length,
    iperf3,
    chromium,
    speedtestLib: lib.version,
    tcpCc,
    sysctls,
    offloads: 'tso, gso and gro off on every lab interface',
    jitterMode,
    netdevMaxBacklog: Number((await must(['sysctl', '-n', 'net.core.netdev_max_backlog'])).trim()),
    weirProfile: 'lab (opt-level s, panic abort, no LTO, 16 codegen units)',
  };
}

async function main() {
  mkdirSync(dirname(out), { recursive: true });
  const shardStart = Date.now();
  if (!Number.isInteger(SHARD) || SHARD < 1 || SHARD > SHARDS || totalText !== String(SHARDS)) throw new Error(`--shard must be N/${SHARDS} with N from 1 to ${SHARDS}`);
  const profiles = selection({
    profiles: flags.get('profiles') || process.env.LAB_PROFILES,
    repeats: flags.get('repeats') || process.env.LAB_REPEATS,
  }).filter((entry) => entry.shard === SHARD);
  if (!profiles.length) {
    log(`shard ${SHARD}: no selected links; nothing to run`);
    return;
  }
  log(`shard ${SHARD}/${totalText}: ${profiles.map((entry) => entry.id).join(', ')}`);
  await runAll(teardown(), true);
  await runAll(topologyUp());
  await runAll(offloads(false));
  const server = started(asRoot(inNs(NS.server, serverBin, '--listen', `${ADDR.server}:${PORT}`, '--static', twinDir, '--wire-iface', IFACE.server)));
  const iperfd = started(asRoot(inNs(NS.server, 'iperf3', '-s', '-J')), 0, true);
  const stop = async () => {
    await iperfd.stop('SIGTERM');
    await server.stop('SIGTERM');
    await runAll(teardown(), true);
  };
  for (const signal of ['SIGINT', 'SIGTERM'] as const) {
    process.once(signal, () => {
      void stop().finally(() => process.exit(130));
    });
  }
  const file: ShardFile = {
    schema: 1,
    shard: SHARD,
    runner: await (async () => {
      await sleep(1000);
      return runnerInfo();
    })(),
    startedAt: new Date(shardStart).toISOString(),
    result: { shard: SHARD, wallMs: 0, cpuModel: '', kernel: '', baseline: null as unknown as Baseline },
    profiles: [],
  };
  file.result.cpuModel = file.runner.cpuModel;
  file.result.kernel = file.runner.kernel;
  file.result.runner = file.runner;
  const save = () => {
    file.result.wallMs = Date.now() - shardStart;
    writeFileSync(out, `${JSON.stringify(file)}\n`);
  };
  try {
    file.result.baseline = await baseline();
    save();
    for (const entry of profiles) {
      file.profiles.push(await runProfile(entry));
      save();
    }
    const findings = JSON.parse(readFileSync(new URL('./findings.json', import.meta.url), 'utf8')) as Finding[];
    const judged: Results = {
      schema: 1, commit: 'shard', weirVersion: '', runUrl: '', startedAt: file.startedAt, runner: file.runner,
      shards: [file.result], profiles: file.profiles,
    };
    for (const id of retestable(slim(judged), findings, profiles)) {
      const entry = profiles.find((item) => item.id === id)!;
      const index = file.profiles.findIndex((item) => item.id === id);
      log(`retest ${id}: it failed a gated check`);
      file.profiles[index] = { ...file.profiles[index]!, retry: await runProfile(entry) };
      save();
    }
  } finally {
    save();
    await stop();
  }
  log(`shard ${SHARD} done in ${Math.round((Date.now() - shardStart) / 1000)} s`);
}

if (import.meta.url === `file://${process.argv[1]}` || process.argv[1]?.endsWith('lab/run.ts')) {
  main().catch(async (error: unknown) => {
    console.error(String(error));
    await runAll(teardown(), true);
    process.exit(1);
  });
}
