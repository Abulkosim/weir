// Pure rendering: docs/accuracy.md and its chart from lab results. Deterministic:
// fixed formatting, profile order from profiles.ts, dates only from the JSON.

import { analyze, fill, type Analysis, type Finding, type Status, type Verdict } from './analyze.ts';
import { netemArgs, tbfBurst, tbfLimit } from './net.ts';
import { ceilingMbps, LEFT_OUT, PROFILES } from './profiles.ts';
import { failed, type Results } from './schema.ts';
import { THRESHOLDS } from './thresholds.ts';

const fmt = (value: number | null | undefined, digits = 1) =>
  typeof value === 'number' && Number.isFinite(value) ? value.toFixed(digits) : 'n/a';
const signed = (value: number | null | undefined, digits = 1) =>
  typeof value === 'number' && Number.isFinite(value) ? `${value > 0 ? '+' : value < 0 ? '−' : '±'}${Math.abs(value).toFixed(digits)}` : 'n/a';

const STATUS_LABEL: Record<Status, string> = {
  pass: 'pass',
  fail: '**fail**',
  xfail: 'expected fail',
  xpass: 'unexpected pass',
  inconclusive: '**inconclusive**',
  'invalid-input': '**invalid input**',
  report: 'report only',
};

function find(analysis: Analysis, profile: string, metric: string): Verdict | undefined {
  return analysis.verdicts.find((item) => item.profile === profile && item.metric === metric);
}

function cell(stat: Verdict['weir'], unit: string, digits = 1): string {
  if (!stat) return '–';
  if (stat.n === 1 || stat.min === stat.max) return `${fmt(stat.median, digits)} ${unit}`;
  return `${fmt(stat.median, digits)} ${unit} (${fmt(stat.min, digits)}–${fmt(stat.max, digits)})`;
}

function errorCell(item: Verdict | undefined, unit: '%' | 'ms'): string {
  if (!item?.error) return '–';
  const { median, min, max, n } = item.error;
  const digits = unit === '%' ? 1 : 1;
  return n > 1 && min !== max ? `${signed(median, digits)} ${unit} (${signed(min, digits)} to ${signed(max, digits)})` : `${signed(median, digits)} ${unit}`;
}

function status(item: Verdict | undefined): string {
  if (!item) return '–';
  return item.finding ? `${STATUS_LABEL[item.status]} ([${item.finding}](#${item.finding.toLowerCase()}))` : STATUS_LABEL[item.status];
}

function shapeLine(id: string, results: Results): string {
  const entry = results.profiles.find((item) => item.id === id);
  const definition = PROFILES.find((item) => item.id === id)!;
  const shape = entry?.shape ?? definition;
  const parts = [
    `${shape.down}/${shape.up} Mbps`,
    `${shape.rttMs} ms RTT`,
    shape.lossPct ? `${shape.lossPct} % loss each way` : null,
    `${shape.bufferMs} ms buffer`,
    shape.jitter ? `jitter ${shape.jitter.ms} ms each way` : null,
    shape.serverMs ? `${shape.serverMs} ms server time` : null,
    shape.uploadTimingIncludesBody ? 'upload Server-Timing includes the body' : null,
  ].filter(Boolean);
  return parts.join(' · ');
}

function profileSection(id: string, results: Results, analysis: Analysis): string {
  const definition = PROFILES.find((item) => item.id === id)!;
  const entry = results.profiles.find((item) => item.id === id);
  const lines = [`### ${id}`, '', `${definition.label}. ${shapeLine(id, results)}.`, ''];
  if (!entry) return [...lines, 'Not in this run.', ''].join('\n');
  const runs = entry.repeats.length;
  const twinRuns = entry.repeats.filter((repeat) => repeat.twin && !failed(repeat.twin)).length;
  lines.push(`${runs} runs of Weir${definition.truth ? ' and iperf3' : ''}${twinRuns ? `, ${twinRuns} of the twin` : ''}. Cells show the median and (range).`, '');
  if (!entry.validity.ok) lines.push(`Lab input check failed: ${entry.validity.reasons.join('; ')}.`, '');
  lines.push('| | Wire | iperf3 | Weir | Twin | Weir error | Status |', '|---|---|---|---|---|---|---|');
  for (const dir of ['download', 'upload'] as const) {
    const wire = find(analysis, id, `${dir}.wire`);
    const capacity = find(analysis, id, `${dir}.capacity`);
    const twin = find(analysis, id, `twin.${dir}`);
    const physics = find(analysis, id, `${dir}.physics`);
    const worst = [wire, capacity, physics].find((item) => item && item.status !== 'pass') ?? capacity ?? wire;
    lines.push(`| ${dir === 'download' ? 'Download' : 'Upload'} | ${cell(wire?.truth ?? null, 'Mbps')} | ${cell(capacity?.truth ?? null, 'Mbps')} | ${cell(wire?.weir ?? capacity?.weir ?? null, 'Mbps')} | ${cell(twin?.truth ?? null, 'Mbps')} | ${errorCell(wire, '%')} wire, ${errorCell(capacity, '%')} iperf3 | ${status(worst)} |`);
  }
  const idle = find(analysis, id, 'idle');
  const twinIdle = find(analysis, id, 'twin.idle');
  lines.push(`| Idle latency | – | ${cell(idle?.truth ?? null, 'ms')} ping | ${cell(idle?.weir ?? null, 'ms')} | ${cell(twinIdle?.truth ?? null, 'ms')} | ${errorCell(idle, 'ms')} | ${status(idle)} |`);
  for (const dir of ['download', 'upload'] as const) {
    const loaded = find(analysis, id, `loaded.${dir}`);
    lines.push(`| Loaded latency, ${dir} | ${cell(loaded?.truth ?? null, 'ms', 0)} ping | – | ${cell(loaded?.weir ?? null, 'ms', 0)} | – | ${errorCell(loaded, 'ms')} | ${status(loaded)} |`);
  }
  const jitter = find(analysis, id, 'jitter');
  lines.push(`| Jitter | – | ${cell(jitter?.truth ?? null, 'ms', 2)} ping | ${cell(jitter?.weir ?? null, 'ms', 2)} | – | – | report only |`);
  const data = find(analysis, id, 'data.ceiling');
  const honesty = find(analysis, id, 'data.honesty');
  lines.push(`| Data used | ${cell(data?.weir ?? null, 'MB', 0)} on the wire | – | ${honesty?.weir ? `counts ${fmt(honesty.weir.median * 100, 0)} % of the payload` : '–'} | – | – | ${status(data)} |`);
  const extras = [find(analysis, id, 'validity'), find(analysis, id, 'bloat'), find(analysis, id, 'qreal.upload'), find(analysis, id, 'coverage')]
    .filter((item): item is Verdict => !!item);
  lines.push('');
  for (const item of extras) lines.push(`- ${item.metric === 'qreal.upload' ? 'Q-real' : item.metric[0]!.toUpperCase() + item.metric.slice(1)}: ${item.detail}. ${status(item)}.`);
  const failing = analysis.verdicts.filter((item) => item.profile === id && item.status !== 'pass' && item.status !== 'report');
  for (const item of failing.filter((verdict) => !extras.includes(verdict))) {
    lines.push(`- ${item.metric}: ${item.detail}.${item.suspect ? ` Suspect: ${item.suspect}.` : ''} ${status(item)}.`);
  }
  return [...lines, ''].join('\n');
}

export function renderMarkdown(results: Results, findings: Finding[], analysis = analyze(results, findings)): string {
  const run = results.runUrl ? `[run](${results.runUrl})` : 'a local run';
  const ids = PROFILES.map((item) => item.id);
  const out: string[] = [
    '# Accuracy',
    '',
    `<!-- Generated by lab/report.ts from docs/accuracy.json. Do not edit; re-run the lab. -->`,
    '',
    `Generated from ${run} on commit \`${results.commit.slice(0, 12)}\` (Weir ${results.weirVersion}), started ${results.startedAt.slice(0, 16).replace('T', ' ')} UTC.`,
    '',
    `Gate: **${analysis.ok ? 'passing' : 'failing'}**. ${Object.entries(analysis.counts).filter(([, count]) => count > 0).map(([name, count]) => `${count} ${STATUS_LABEL[name as Status].replace(/\*/g, '')}`).join(', ')}.`,
    '',
    '<picture>',
    '  <source media="(prefers-color-scheme: dark)" srcset="images/accuracy-dark.svg">',
    '  <img alt="Weir\'s error against iperf3 per shaped link, and its idle and loaded latency against ping." src="images/accuracy-light.svg" width="880">',
    '</picture>',
    '',
    '## How it works',
    '',
    'Every engine change runs the real Weir engine (`src-tauri/examples/lab.rs`, the shipped `speedtest::run` with the Cloudflare backend) against a local server on a link shaped on a GitHub-hosted Ubuntu runner.',
    '',
    '- **Topology.** Three Linux network namespaces: client, router, server, joined by veth pairs. All shaping sits on the router\'s two egresses, so the bottleneck queue is a real tail-drop FIFO and not the sender\'s own TCP Small Queues.',
    '- **Server.** `lab/server`, a small hyper server that speaks the part of Cloudflare\'s protocol Weir uses (`/cdn-cgi/trace`, `/__down?bytes=N`, `/__up`, `Server-Timing`). It shares no code with Weir.',
    '- **Shaping.** netem adds delay (and loss, and jitter) before a tbf rate limiter whose byte limit is the buffer depth. Offloads (TSO, GSO, GRO) are off, so loss hits single packets and wire counters are per frame.',
    `- **Example, gig-5 download direction:** \`tc qdisc add dev rc root handle 1: ${netemArgs(PROFILES.find((item) => item.id === 'gig-5')!, 'pareto').join(' ')}\` then \`tc qdisc add dev rc parent 1:1 handle 10: tbf rate 1000mbit burst ${tbfBurst(1000)} limit ${tbfLimit(1000, 10)}\`.`,
    `- **Jitter** uses netem's paretonormal delay with \`rate 10gbit\`, which keeps packets in order. Loss applies in each direction, to data and ACKs alike.`,
    '',
    'Four layers of truth, each answering a different question:',
    '',
    '| Layer | Truth | Question it answers |',
    '|---|---|---|',
    '| Wire (same run) | Client interface counters (download) and server interface counters (upload) over Weir\'s own steady window, as TCP payload; a concurrent ping in Weir\'s own windows | Is Weir\'s math and counting right for the load it generated? |',
    '| Capacity (interleaved) | iperf3 with the same stream count, in the same repeat | Does Weir\'s HTTP load fill the link as well as bulk TCP? |',
    '| Physics | The configured shape | Is anything impossible? |',
    '| Twin | Cloudflare\'s own `@cloudflare/speedtest` in headless Chromium | Does a different method agree? Report only. |',
    '',
    'Repeats are interleaved and rotate their order (iperf3, Weir, twin; then Weir, twin, iperf3; …). Every step waits for the queues to drain. A step with any CPU over 95 % busy is retried once.',
    '',
    '## Thresholds',
    '',
    'Pre-registered before the first run. A failure is a finding, not a reason to move a number.',
    '',
    '| Check | Limit | Reason |',
    '|---|---|---|',
    ...Object.entries(THRESHOLDS).map(([name, limits]) => {
      const values = Object.entries(limits).filter(([key]) => key !== 'reason').map(([key, value]) => `${key} ${value}`).join(', ');
      return `| ${name} | ${values} | ${limits.reason} |`;
    }),
    '',
    '## Summary',
    '',
    '| Link | Download vs iperf3 | Upload vs iperf3 | Idle latency | Loaded latency | Data | Status |',
    '|---|---|---|---|---|---|---|',
  ];
  for (const id of ids) {
    const verdicts = analysis.verdicts.filter((item) => item.profile === id);
    const worst = verdicts.find((item) => !['pass', 'report'].includes(item.status));
    const loadedDown = find(analysis, id, 'loaded.download');
    const loadedUp = find(analysis, id, 'loaded.upload');
    out.push(`| [${id}](#${id.replace(/\./g, '')}) | ${errorCell(find(analysis, id, 'download.capacity'), '%')} | ${errorCell(find(analysis, id, 'upload.capacity'), '%')} | ${errorCell(find(analysis, id, 'idle'), 'ms')} | ${errorCell(loadedDown, 'ms')} / ${errorCell(loadedUp, 'ms')} | ${cell(find(analysis, id, 'data.ceiling')?.weir ?? null, 'MB', 0)} | ${worst ? `${STATUS_LABEL[worst.status]} (${worst.metric})` : 'pass'} |`);
  }
  out.push('', 'Errors are Weir minus the truth: percent of iperf3 for speeds, ms against a ping for latency.', '', '## Links', '');
  for (const id of ids) out.push(profileSection(id, results, analysis));

  out.push('## Baseline', '', 'Each shard first runs unshaped, to prove the server, the client host and the router are not the bottleneck at 1 Gbps.', '', '| Shard | CPU | iperf3 | Weir | Twin | Server | Softnet drops | Loaded latency inflation |', '|---|---|---|---|---|---|---|---|');
  for (const shard of results.shards) {
    const b = shard.baseline;
    if (!b) {
      out.push(`| ${shard.shard} | ${shard.cpuModel} | – | – | – | – | – | – |`);
      continue;
    }
    out.push(`| ${shard.shard} | ${shard.cpuModel} | ${fmt(b.iperfDown, 0)}/${fmt(b.iperfUp, 0)} Mbps | ${fmt(b.weirDown, 0)}/${fmt(b.weirUp, 0)} Mbps${b.weirError ? ` (${b.weirError})` : ''} | ${fmt(b.twinDown, 0)}/${fmt(b.twinUp, 0)} Mbps | ${fmt(b.serverCpuCores, 2)} cores at ${fmt(b.serverGbps, 2)} Gbps | ${b.softnetDrops} | ${fmt(b.weirLoadedInflationMs, 2)} ms |`);
  }
  const baselineVerdicts = analysis.verdicts.filter((item) => item.metric === 'baseline' || item.profile.startsWith('baseline-'));
  const notPassing = baselineVerdicts.filter((item) => item.status !== 'pass');
  out.push('', notPassing.length ? notPassing.map((item) => `- ${item.profile}: ${item.detail}. ${status(item)}.`).join('\n') : 'Every baseline check passes.', '');

  out.push('## Findings', '');
  if (!findings.length) out.push('None open.', '');
  for (const finding of findings) {
    const covered = finding.expect.map((pair) => `${pair.profile} ${pair.metric}`).join(', ');
    out.push(`### ${finding.id}`, '', `**${finding.title}** (${finding.status}). Where: \`${finding.where}\`.`, '', fill(finding.cause, analysis.facts), '');
    if (covered) out.push(`Expected to fail until fixed: ${covered}.`, '');
  }

  out.push('## Claim check: Cloudflare\'s own test', '', 'docs/method.md says Weir and Cloudflare\'s test agree within about 10 % on links up to about 300 Mbps. The twin runs `@cloudflare/speedtest` ' + results.runner.speedtestLib + ' with its default measurements against the same server; it is never gated. The twin reports the 90th percentile of single requests, so it differs from Weir by design.', '', '| Link | Download, Weir vs twin | Upload, Weir vs twin | Within 10 % |', '|---|---|---|---|');
  for (const id of ids) {
    const definition = PROFILES.find((item) => item.id === id)!;
    if (definition.down > 300) continue;
    const down = find(analysis, id, 'twin.download');
    const up = find(analysis, id, 'twin.upload');
    if (!down?.error && !up?.error) continue;
    const within = [down, up].every((item) => item?.error && Math.abs(item.error.median) <= 10);
    out.push(`| ${id} | ${signed(down?.error?.median)} % | ${signed(up?.error?.median)} % | ${within ? 'yes' : 'no'} |`);
  }
  out.push('');

  const qreal = find(analysis, 'qreal-up', 'qreal.upload');
  out.push('## Q-real: does Cloudflare\'s upload Server-Timing include the body?', '',
    'Weir subtracts the upload\'s server time from each request (`cloudflare.rs`); the twin does not. If Cloudflare\'s `__up` server time covered the body transfer, Weir would read high. The `qreal-up` link makes the lab server do exactly that.', '',
    qreal ? `${qreal.detail}. Settling it needs the owner's real-network check (plan 1.6).` : 'Not in this run.', '');

  out.push('## What the lab leaves out', '', ...LEFT_OUT.map((line) => `- ${line}`), '');
  out.push('## Run it yourself', '', 'Linux with sudo, iperf3 ≥ 3.16, ethtool, the `sch_netem` and `sch_tbf` modules, Node 24 and pnpm. It creates three network namespaces and removes them when done.', '', '```sh',
    'pnpm install --frozen-lockfile && pnpm build && pnpm exec playwright install --only-shell chromium',
    'WEIR_LAB=1 cargo build --manifest-path src-tauri/Cargo.toml --profile lab --features lab --example lab --locked',
    'cargo build --manifest-path lab/server/Cargo.toml --release --locked && pnpm lab:twin',
    'node lab/run.ts --shard 1/5 --weir src-tauri/target/lab/examples/lab --server lab/server/target/release/weir-lab-server --twin lab-out/twin --out lab-out/shards/shard-1.json',
    'node lab/report.ts --in lab-out/shards --out lab-out/report',
    '```', '',
    `Runner for this run: ${results.runner.cpus} CPUs (${results.runner.cpuModel}), kernel ${results.runner.kernel}, iperf3 ${results.runner.iperf3}, ${results.runner.tcpCc}, netdev backlog ${results.runner.netdevMaxBacklog}, ${results.runner.offloads}. Weir built with the ${results.runner.weirProfile} profile; shipped builds also use LTO and one codegen unit.`, '');
  return `${out.join('\n').replace(/\n{3,}/g, '\n\n').trimEnd()}\n`;
}

export function renderSummary(results: Results, analysis: Analysis): string {
  const lines = [`## Accuracy lab: ${analysis.ok ? 'passing' : 'failing'}`, '', Object.entries(analysis.counts).map(([name, count]) => `${name} ${count}`).join(' · '), ''];
  const notable = analysis.verdicts.filter((item) => !['pass', 'report'].includes(item.status));
  if (notable.length) {
    lines.push('| Link | Check | Status | Suspect | Detail |', '|---|---|---|---|---|');
    for (const item of notable) lines.push(`| ${item.profile} | ${item.metric} | ${item.status}${item.finding ? ` (${item.finding})` : ''} | ${item.suspect ?? ''} | ${item.detail} |`);
  }
  lines.push('', `Shard wall times: ${results.shards.map((shard) => `${shard.shard}: ${Math.round(shard.wallMs / 60000)} min`).join(', ')}.`, '');
  return `${lines.join('\n')}\n`;
}

interface Theme {
  surface: string;
  ink: string;
  secondary: string;
  tertiary: string;
  grid: string;
  band: string;
  accent: string;
  danger: string;
}

const THEMES: Record<'light' | 'dark', Theme> = {
  light: { surface: '#f6f9fa', ink: '#0e1b22', secondary: '#4a5b64', tertiary: '#62727a', grid: '#e2eaed', band: '#dcf1ee', accent: '#0a8f88', danger: '#b4332b' },
  dark: { surface: '#0e151a', ink: '#e7f1f3', secondary: '#a7b8bf', tertiary: '#8597a0', grid: '#22313a', band: '#143331', accent: '#3fd6c8', danger: '#ff8f84' },
};

const escape = (text: string) => text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
const n = (value: number) => Number(value.toFixed(1)).toString();

export function renderSvg(results: Results, analysis: Analysis, mode: 'light' | 'dark'): string {
  const theme = THEMES[mode];
  const ids = PROFILES.map((item) => item.id);
  const width = 880;
  const top = 64;
  const row = 28;
  const height = top + ids.length * row + 44;
  const left = { x0: 150, x1: 470, min: -40, max: 40 };
  const right = { x0: 560, x1: 860, min: 1, max: 2000 };
  const xErr = (value: number) => left.x0 + ((Math.max(left.min, Math.min(left.max, value)) - left.min) / (left.max - left.min)) * (left.x1 - left.x0);
  const xMs = (value: number) => right.x0 + (Math.log10(Math.max(right.min, Math.min(right.max, value))) / Math.log10(right.max)) * (right.x1 - right.x0);
  const parts: string[] = [];
  const ticks: string[] = [];
  const text = (x: number, y: number, body: string, attrs = '') => parts.push(`<text x="${n(x)}" y="${n(y)}"${attrs}>${escape(body)}</text>`);

  parts.push(`<rect width="${width}" height="${height}" rx="10" fill="${theme.surface}"/>`);
  text(16, 26, 'Weir against the truth, per shaped link', ` class="title"`);
  text(left.x0, 50, 'Speed error vs iperf3 (%) · ● download ○ upload · | wire', ' class="axis-title"');
  text(right.x0, 50, 'Latency (ms, log) · ● Weir | ping · idle and loaded', ' class="axis-title"');

  for (const tick of [-40, -20, 0, 20, 40]) {
    const x = xErr(tick);
    parts.push(`<line x1="${n(x)}" y1="${top - 6}" x2="${n(x)}" y2="${top + ids.length * row}" stroke="${theme.grid}" stroke-width="1"/>`);
    text(x, top + ids.length * row + 16, `${tick > 0 ? '+' : tick < 0 ? '−' : ''}${Math.abs(tick)} %`, ' class="tick" text-anchor="middle"');
  }
  for (const tick of [1, 10, 100, 1000]) {
    const x = xMs(tick);
    parts.push(`<line x1="${n(x)}" y1="${top - 6}" x2="${n(x)}" y2="${top + ids.length * row}" stroke="${theme.grid}" stroke-width="1"/>`);
    text(x, top + ids.length * row + 16, `${tick} ms`, ' class="tick" text-anchor="middle"');
  }

  ids.forEach((id, index) => {
    parts.push(...ticks.splice(0));
    const y = top + index * row + row / 2;
    const definition = PROFILES.find((item) => item.id === id)!;
    const entry = results.profiles.find((item) => item.id === id);
    text(16, y + 4, id, ' class="label"');
    const noisy = definition.lossPct > 0 || !!definition.jitter;
    const band = noisy
      ? [-THRESHOLDS.capacityNoisy.medianAbsPct, THRESHOLDS.capacityNoisy.medianAbsPct]
      : [THRESHOLDS.capacityClean.medianMinPct, THRESHOLDS.capacityClean.medianMaxPct];
    if (!definition.reportOnly) {
      parts.push(`<rect x="${n(xErr(band[0]!))}" y="${n(y - 8)}" width="${n(xErr(band[1]!) - xErr(band[0]!))}" height="16" rx="4" fill="${theme.band}"/>`);
    }
    if (!entry) {
      text(left.x0 + 6, y + 4, 'not in this run', ' class="note"');
      return;
    }
    for (const [dir, offset] of [['download', -5], ['upload', 5]] as const) {
      const capacity = find(analysis, id, `${dir}.capacity`);
      const wire = find(analysis, id, `${dir}.wire`);
      const bad = capacity && !['pass', 'report', 'xpass'].includes(capacity.status);
      const colour = bad ? theme.danger : theme.accent;
      if (wire?.error && capacity?.truth && wire.truth) {
        const wireVsTruth = ((wire.truth.median - capacity.truth.median) / capacity.truth.median) * 100;
        const x = xErr(wireVsTruth);
        ticks.push(`<line x1="${n(x)}" y1="${n(y + offset - 5)}" x2="${n(x)}" y2="${n(y + offset + 5)}" stroke="${theme.ink}" stroke-width="1.5"/>`);
      }
      if (capacity?.error) {
        const x = xErr(capacity.error.median);
        const title = `<title>${escape(`${id} ${dir}: ${signed(capacity.error.median)} % vs iperf3`)}</title>`;
        parts.push(dir === 'download'
          ? `<circle cx="${n(x)}" cy="${n(y + offset)}" r="4.5" fill="${colour}" stroke="${theme.surface}" stroke-width="2">${title}</circle>`
          : `<circle cx="${n(x)}" cy="${n(y + offset)}" r="3.5" fill="${theme.surface}" stroke="${colour}" stroke-width="2">${title}</circle>`);
      }
    }
    const idle = find(analysis, id, 'idle');
    const loaded = find(analysis, id, 'loaded.download');
    for (const item of [idle, loaded]) {
      if (!item) continue;
      const bad = !['pass', 'report', 'xpass'].includes(item.status);
      if (item.truth) {
        const x = xMs(item.truth.median);
        ticks.push(`<line x1="${n(x)}" y1="${n(y - 10)}" x2="${n(x)}" y2="${n(y + 10)}" stroke="${theme.ink}" stroke-width="1.5"/>`);
      }
      if (item.weir) {
        parts.push(`<circle cx="${n(xMs(item.weir.median))}" cy="${n(y)}" r="4.5" fill="${bad ? theme.danger : theme.accent}" stroke="${theme.surface}" stroke-width="2"><title>${escape(`${id} ${item.metric}: Weir ${fmt(item.weir.median)} ms, ping ${fmt(item.truth?.median)} ms`)}</title></circle>`);
      }
    }
  });
  parts.push(...ticks.splice(0));
  text(16, height - 12, `Shaded: pass band for the median. Red: not passing. ${results.commit.slice(0, 12)} · ${results.startedAt.slice(0, 10)}`, ' class="note"');

  const font = '-apple-system, BlinkMacSystemFont, &quot;Segoe UI&quot;, system-ui, sans-serif';
  const style = `text{font-family:${font};font-variant-numeric:tabular-nums;fill:${theme.secondary};font-size:11px}.title{fill:${theme.ink};font-size:14px;font-weight:600}.axis-title{fill:${theme.secondary};font-size:11px}.label{fill:${theme.ink};font-size:12px}.tick{fill:${theme.tertiary};font-size:10px}.note{fill:${theme.tertiary};font-size:10px}`;
  const ceilingNote = `${ids.length} links; ceiling for 1 Gbps is ${fmt(ceilingMbps(1000), 0)} Mbps`;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-label="${escape(`Weir accuracy per shaped link, ${ceilingNote}`)}"><style>${style}</style>${parts.join('')}</svg>\n`;
}

export function renderAll(results: Results, findings: Finding[]): Record<string, string> {
  const analysis = analyze(results, findings);
  return {
    'accuracy.md': renderMarkdown(results, findings, analysis),
    'accuracy.json': `${JSON.stringify(results, null, 1)}\n`,
    'images/accuracy-light.svg': renderSvg(results, analysis, 'light'),
    'images/accuracy-dark.svg': renderSvg(results, analysis, 'dark'),
  };
}
