// Runner capability check: can this host shape links and sustain 1 Gbps?
// node lab/spike.ts --server <bin> --static <dir> --out <file.json>
// Prints numbers only (no addresses or hostnames).

import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import {
  ADDR, IFACE, NS, PORT, inNs, offloads, parseNstat, parseQdiscStats, parseSoftnet, qdiscStats, reorderDelta, shape,
  teardown, topologyUp, unshape, type JitterMode, type Shape,
} from './net.ts';
import { asRoot, asUser, clientUser, must, run, runAll, sleep, started, type Background } from './exec.ts';
import { cpuMax, median, parseIperf, parsePidStat, parsePing, parseProcStat, percentile } from './parse.ts';

const args = new Map<string, string>();
for (let index = 2; index < process.argv.length; index += 2) args.set(process.argv[index]!.replace(/^--/, ''), process.argv[index + 1] ?? '');
const serverBin = args.get('server') ?? 'lab/server/target/release/weir-lab-server';
const staticDir = args.get('static') ?? 'lab-out/twin';
const out = args.get('out') ?? 'lab-out/spike.json';
const base = `http://${ADDR.server}:${PORT}`;

const results: Record<string, unknown> = {};
const notes: string[] = [];
const note = (line: string) => {
  notes.push(line);
  console.log(line);
};

async function cpuSnapshot() {
  return parseProcStat(readFileSync('/proc/stat', 'utf8'));
}

async function softnet() {
  return parseSoftnet(readFileSync('/proc/net/softnet_stat', 'utf8')).dropped;
}

async function iperf(dir: 'down' | 'up', streams: number, seconds = 8) {
  const before = await cpuSnapshot();
  const drops = await softnet();
  const argv = ['iperf3', '-c', ADDR.server, '-P', String(streams), '-t', String(seconds), '-O', '2', '-J'];
  argv.push(dir === 'down' ? '-R' : '--get-server-output');
  const done = await run(clientUser(argv), (seconds + 20) * 1000);
  const cpu = cpuMax(before, await cpuSnapshot());
  const dropped = (await softnet()) - drops;
  if (done.code !== 0) return { error: done.timedOut ? 'timeout' : `exit ${done.code}`, cpuMax: cpu, softnetDrops: dropped };
  try {
    const parsed = parseIperf(done.stdout);
    return { mbps: round(parsed.mbps), endMbps: round(parsed.endMbps), mismatch: round(parsed.mismatch, 4), retransmits: parsed.retransmits, cpuMax: round(cpu, 3), softnetDrops: dropped };
  } catch (error) {
    return { error: String(error), cpuMax: cpu, softnetDrops: dropped };
  }
}

function pinger(intervalS: number): Background {
  return started(asRoot(inNs(NS.client, 'ping', '-D', '-O', '-n', '-i', String(intervalS), ADDR.server)));
}

async function pingSummary(seconds: number, intervalS = 0.05) {
  const ping = pinger(intervalS);
  await sleep(seconds * 1000);
  const done = await ping.stop();
  const samples = parsePing(done.stdout, intervalS * 1000).slice(1).map((sample) => sample.rttMs);
  return summary(samples);
}

function summary(samples: number[]) {
  return {
    n: samples.length,
    p5: round(percentile(samples, 0.05)),
    p50: round(median(samples)),
    p95: round(percentile(samples, 0.95)),
  };
}

async function loadedPing(dir: 'down' | 'up', streams: number) {
  const ping = pinger(0.1);
  const startedAt = Date.now();
  const load = await iperf(dir, streams, 8);
  const done = await ping.stop();
  const samples = parsePing(done.stdout, 100)
    .filter((sample) => sample.sentMs >= startedAt + 4000 && sample.sentMs < startedAt + 10_000)
    .map((sample) => sample.rttMs);
  return { load, ping: summary(samples) };
}

const round = (value: number | null, digits = 2) => (value === null ? null : Number(value.toFixed(digits)));

async function setShape(value: Shape | null, mode: JitterMode = 'pareto') {
  await runAll(unshape(), true);
  if (value) await runAll(shape(value, mode));
  await sleep(500);
}

async function qdisc(dev: string) {
  return parseQdiscStats(await must(asRoot(qdiscStats(dev))));
}

async function nstat(ns: string) {
  return parseNstat(await must(asRoot(inNs(ns, 'nstat', '-asz'))));
}

async function serverCpuTicks(): Promise<number> {
  const pid = (await run(['pgrep', '-x', 'weir-lab-server'])).stdout.trim().split('\n')[0];
  if (!pid) return 0;
  return parsePidStat(readFileSync(`/proc/${pid}/stat`, 'utf8'));
}

async function curlLoad(dir: 'down' | 'up', parallel: number, bytes: number) {
  const ticks = await serverCpuTicks();
  const begin = Date.now();
  const jobs = Array.from({ length: parallel }, () => {
    const argv = dir === 'down'
      ? ['curl', '-s', '-o', '/dev/null', '-w', '%{size_download}', `${base}/__down?bytes=${bytes}`]
      : ['curl', '-s', '-o', '/dev/null', '-w', '%{size_upload}', '-X', 'POST', '-H', 'content-type: application/octet-stream', '-T', 'lab-out/zero.bin', `${base}/__up`];
    return run(clientUser(argv), 120_000);
  });
  const done = await Promise.all(jobs);
  const ms = Date.now() - begin;
  const moved = done.reduce((sum, job) => sum + Number(job.stdout.trim() || 0), 0);
  const cores = ((await serverCpuTicks()) - ticks) / 100 / (ms / 1000);
  return { mbps: round((moved * 8) / ms / 1000), seconds: round(ms / 1000), serverCores: round(cores), failures: done.filter((job) => job.code !== 0).length };
}

async function info() {
  const kernel = (await must(['uname', '-r'])).trim();
  let hz: string | null = null;
  try {
    hz = /^CONFIG_HZ=(\d+)/m.exec(readFileSync(`/boot/config-${kernel}`, 'utf8'))?.[1] ?? null;
  } catch {
    hz = null;
  }
  const cpuModel = /model name\s*:\s*(.*)/.exec(readFileSync('/proc/cpuinfo', 'utf8'))?.[1] ?? 'unknown';
  const iperf3 = /iperf (\S+)/.exec((await run(['iperf3', '--version'])).stdout)?.[1] ?? 'missing';
  return { kernel, hz, cpuModel, cpus: parseProcStat(readFileSync('/proc/stat', 'utf8')).length, iperf3 };
}

async function main() {
  mkdirSync(dirname(out), { recursive: true });
  results.runner = await info();
  note(`runner ${JSON.stringify(results.runner)}`);
  await runAll(teardown(), true);
  await runAll(topologyUp());
  const backlogDefault = (await must(['sysctl', '-n', 'net.core.netdev_max_backlog'])).trim();
  results.backlogDefault = Number(backlogDefault);
  await runAll(offloads(false));
  await must(['truncate', '-s', '1000000000', 'lab-out/zero.bin']);

  const server = started(asRoot(inNs(NS.server, serverBin, '--listen', `${ADDR.server}:${PORT}`, '--static', staticDir, '--wire-iface', IFACE.server)));
  const iperfd = started(asRoot(inNs(NS.server, 'iperf3', '-s')));
  try {
    await sleep(1500);
    const trace = await run(clientUser(['curl', '-s', `${base}/cdn-cgi/trace`]));
    results.serverUp = trace.stdout.includes('colo=LAB');
    note(`server up: ${results.serverUp}`);

    const unshaped: Record<string, unknown> = {};
    for (const backlog of [backlogDefault, '10000']) {
      await must(asRoot(['sysctl', '-q', '-w', `net.core.netdev_max_backlog=${backlog}`]));
      for (const on of [false, true]) {
        await runAll(offloads(on));
        for (const dir of ['down', 'up'] as const) {
          const key = `backlog${backlog}-offloads${on ? 'On' : 'Off'}-${dir}`;
          unshaped[key] = await iperf(dir, 8);
          note(`unshaped ${key}: ${JSON.stringify(unshaped[key])}`);
        }
      }
    }
    results.unshaped = unshaped;
    await runAll(offloads(false));

    const curl: Record<string, unknown> = {};
    curl.down = await curlLoad('down', 6, 1_000_000_000);
    curl.up = await curlLoad('up', 6, 1_000_000_000);
    note(`server unshaped curl: ${JSON.stringify(curl)}`);

    const gig5: Shape = { down: 1000, up: 1000, rttMs: 5, lossPct: 0, bufferMs: 10 };
    await setShape(gig5);
    const gig: Record<string, unknown> = { idle: await pingSummary(2) };
    gig.down = await iperf('down', 6);
    gig.up = await iperf('up', 6);
    gig.curlDown = await curlLoad('down', 6, 300_000_000);
    gig.curlUp = await curlLoad('up', 6, 1_000_000_000);
    gig.qdiscRc = await qdisc(IFACE.routerToClient);
    results.gig5 = gig;
    note(`gig-5: ${JSON.stringify(gig)}`);

    for (const bufferMs of [600, 20]) {
      const dsl: Shape = { down: 25, up: 5, rttMs: 30, lossPct: 0, bufferMs };
      await setShape(dsl);
      const entry = { idle: await pingSummary(2), down: await loadedPing('down', 6), up: await loadedPing('up', 4) };
      results[`dsl25-buffer${bufferMs}`] = entry;
      note(`dsl-25 buffer ${bufferMs} ms: ${JSON.stringify(entry)}`);
    }

    const jitterShapes: Array<[string, Shape]> = [
      ['wifi', { down: 200, up: 50, rttMs: 15, lossPct: 0, bufferMs: 50, jitter: { ms: 3 } }],
      ['mobile', { down: 30, up: 8, rttMs: 50, lossPct: 0, bufferMs: 300, jitter: { ms: 6 } }],
    ];
    for (const [name, value] of jitterShapes) {
      for (const mode of ['slot', 'pareto'] as const) {
        await setShape(value, mode);
        const before = { c: await nstat(NS.client), s: await nstat(NS.server) };
        const idle = await pingSummary(4, 0.02);
        const down = await loadedPing('down', 6);
        const up = await loadedPing('up', 4);
        const reorder = reorderDelta(before.c, await nstat(NS.client)) + reorderDelta(before.s, await nstat(NS.server));
        const entry = { idle, down, up, reorder };
        results[`jitter-${name}-${mode}`] = entry;
        note(`jitter ${name} ${mode}: ${JSON.stringify(entry)}`);
      }
    }

    const lossy: Shape = { down: 100, up: 20, rttMs: 40, lossPct: 1, bufferMs: 50 };
    await setShape(lossy);
    const lossyEntry: Record<string, unknown> = { down: await iperf('down', 6), up: await iperf('up', 4) };
    for (const dev of [IFACE.routerToClient, IFACE.routerToServer]) {
      const netem = (await qdisc(dev)).find((entry) => entry.kind === 'netem');
      lossyEntry[dev] = netem ? { lossFraction: round(netem.drops / (netem.packets + netem.drops), 4), packets: netem.packets } : null;
    }
    results.lossy1 = lossyEntry;
    note(`lossy-1: ${JSON.stringify(lossyEntry)}`);

    await setShape(null);
    const browser = await run(asUser(NS.client, ['node', 'lab/twin.ts', '--smoke', '--url', `${base}/twin/index.html`]), 60_000);
    results.chromium = { code: browser.code, out: browser.stdout.trim().slice(0, 300), err: browser.stderr.trim().slice(-300) };
    note(`chromium in netns: ${JSON.stringify(results.chromium)}`);
  } finally {
    await iperfd.stop('SIGTERM');
    await server.stop('SIGTERM');
    await runAll(teardown(), true);
    await run(['rm', '-f', 'lab-out/zero.bin']);
  }
  writeFileSync(out, `${JSON.stringify(results, null, 2)}\n`);
  if (process.env.GITHUB_STEP_SUMMARY) {
    appendFileSync(process.env.GITHUB_STEP_SUMMARY, `## Lab spike\n\n\`\`\`\n${notes.join('\n')}\n\`\`\`\n`);
  }
}

main().catch(async (error: unknown) => {
  console.error(String(error));
  await runAll(teardown(), true);
  process.exit(1);
});
