// Pure judging: lab results in, verdicts out. No I/O and no clocks.

import { median, percentile } from './parse.ts';
import { ceilingMbps, expectedFar, PROFILES, type Profile } from './profiles.ts';
import { failed, type Baseline, type ProfileResult, type Repeat, type Results, type Truth, type Twin, type Validity, type Weir } from './schema.ts';
import { THRESHOLDS } from './thresholds.ts';

export type Status = 'pass' | 'fail' | 'xfail' | 'xpass' | 'inconclusive' | 'invalid-input' | 'report';
export type Suspect = 'input' | 'judge' | 'weir-math' | 'weir-load' | 'unclear';

export const GATED_OK: ReadonlySet<Status> = new Set(['pass', 'xfail', 'xpass', 'report']);

export const METRICS = [
  'validity',
  'download.wire',
  'upload.wire',
  'download.capacity',
  'upload.capacity',
  'download.physics',
  'upload.physics',
  'idle',
  'loaded.download',
  'loaded.upload',
  'bloat',
  'data.ceiling',
  'data.honesty',
  'jitter',
  'loaded.iperf',
  'twin.download',
  'twin.upload',
  'twin.idle',
  'qreal.upload',
  'baseline',
  'coverage',
] as const;
export type Metric = (typeof METRICS)[number];

export const REPORT_ONLY: ReadonlySet<Metric> = new Set([
  'data.honesty', 'jitter', 'loaded.iperf', 'twin.download', 'twin.upload', 'twin.idle', 'qreal.upload',
]);

export interface Stat {
  median: number;
  min: number;
  max: number;
  n: number;
}

export interface Verdict {
  profile: string;
  metric: Metric;
  status: Status;
  suspect: Suspect | null;
  finding: string | null;
  detail: string;
  weir: Stat | null;
  truth: Stat | null;
  /** Per-repeat error in percent (or ms for latency metrics). */
  error: Stat | null;
}

export interface Finding {
  id: string;
  title: string;
  status: 'open' | 'fixed';
  where: string;
  cause: string;
  expect: Array<{ profile: string; metric: Metric }>;
}

export interface Analysis {
  verdicts: Verdict[];
  ok: boolean;
  counts: Record<Status, number>;
  facts: Record<string, Record<string, string>>;
}

export function stat(values: Array<number | null | undefined>): Stat | null {
  const clean = values.filter((value): value is number => typeof value === 'number' && Number.isFinite(value));
  if (!clean.length) return null;
  return { median: median(clean)!, min: Math.min(...clean), max: Math.max(...clean), n: clean.length };
}

const weirOf = (repeat: Repeat): Weir | null => (failed(repeat.weir) ? null : repeat.weir);
const truthOf = (repeat: Repeat): Truth | null => (repeat.truth && !failed(repeat.truth) ? repeat.truth : null);
const twinOf = (repeat: Repeat): Twin | null => (repeat.twin && !failed(repeat.twin) ? repeat.twin : null);

/** Truth a judge can stand on: not saturated and iperf3's own sums agree. */
export function usableTruth(truth: Truth | null): truth is Truth {
  return !!truth && !truth.saturated && truth.mismatch.down <= 0.01 && truth.mismatch.up <= 0.01;
}

const pct = (value: number, reference: number) => ((value - reference) / reference) * 100;
const fmt = (value: number | null | undefined, digits = 1) =>
  typeof value === 'number' && Number.isFinite(value) ? value.toFixed(digits) : 'n/a';

/** truth (max − min) / median, as a fraction. */
export function noise(values: number[]): number {
  const summary = stat(values);
  if (!summary || summary.median === 0) return 0;
  return (summary.max - summary.min) / summary.median;
}

export function isNoisy(shape: { lossPct: number; jitter?: unknown }): boolean {
  return shape.lossPct > 0 || !!shape.jitter;
}

interface Pairs {
  weir: Array<number | null>;
  other: Array<number | null>;
}

function pairs(repeats: Repeat[], weir: (w: Weir) => number | null | undefined, other: (r: Repeat, w: Weir) => number | null | undefined): Pairs {
  const out: Pairs = { weir: [], other: [] };
  for (const repeat of repeats) {
    const w = weirOf(repeat);
    if (!w?.report) continue;
    out.weir.push(weir(w) ?? null);
    out.other.push(other(repeat, w) ?? null);
  }
  return out;
}

function errors(values: Pairs, mode: 'pct' | 'ms'): number[] {
  const result: number[] = [];
  values.weir.forEach((weir, index) => {
    const other = values.other[index];
    if (typeof weir !== 'number' || typeof other !== 'number' || (mode === 'pct' && other <= 0)) return;
    result.push(mode === 'pct' ? pct(weir, other) : weir - other);
  });
  return result;
}

function verdict(profile: string, metric: Metric, status: Status, detail: string, extra: Partial<Verdict> = {}): Verdict {
  return { profile, metric, status, suspect: null, finding: null, detail, weir: null, truth: null, error: null, ...extra };
}

function weirFailures(entry: ProfileResult): string[] {
  const messages: string[] = [];
  for (const repeat of entry.repeats) {
    if (failed(repeat.weir)) messages.push(repeat.weir.error);
    else if (repeat.weir.error) messages.push(repeat.weir.error);
    else if (repeat.weir.report?.uploadError) messages.push(`upload ${repeat.weir.report.uploadError}`);
  }
  return messages;
}

function countWords(messages: string[]): string {
  const counts = new Map<string, number>();
  for (const message of messages) counts.set(message, (counts.get(message) ?? 0) + 1);
  return [...counts].map(([message, count]) => `${message} ×${count}`).join(', ');
}

function profileVerdicts(entry: ProfileResult, all: ProfileResult[], twinCeilingMbps: number | null): Verdict[] {
  const out: Verdict[] = [];
  const { repeats, shape } = entry;
  const noisy = isNoisy(shape);
  const id = entry.id;
  const anySaturated = repeats.some((repeat) => weirOf(repeat)?.saturated);

  const failures = weirFailures(entry);
  const timeoutsOnly = failures.length > 0 && failures.every((message) => /^(timeout|exit|unreadable)/.test(message));
  out.push(
    failures.length
      ? verdict(id, 'validity', 'fail', `Weir did not finish every run: ${countWords(failures)} in ${repeats.length} runs`, {
          suspect: failures.some((message) => /tooShort|stalled/.test(message)) ? 'weir-math' : timeoutsOnly ? 'judge' : 'unclear',
        })
      : verdict(id, 'validity', 'pass', `${repeats.length} of ${repeats.length} runs finished with a download and an upload`),
  );
  const validitySuspect = out[0]!.suspect;

  for (const dir of ['download', 'upload'] as const) {
    const band = dir === 'download' ? THRESHOLDS.downloadWire.maxAbsPct : THRESHOLDS.uploadWire.maxAbsPct;
    const speed = (w: Weir) => (dir === 'download' ? w.report?.download.mbps : w.report?.upload?.mbps);
    const wire = pairs(repeats, speed, (_r, w) => (dir === 'download' ? w.wire.downMbps : w.wire.upMbps));
    const wireErrors = errors(wire, 'pct');
    const metric: Metric = `${dir}.wire`;
    if (!wireErrors.length) {
      out.push(verdict(id, metric, 'fail', 'no Weir result to compare with the wire', { suspect: validitySuspect ?? 'unclear' }));
    } else {
      const worst = Math.max(...wireErrors.map(Math.abs));
      const ok = worst <= band;
      out.push(verdict(id, metric, anySaturated && !ok ? 'inconclusive' : ok ? 'pass' : 'fail',
        `worst ${fmt(worst)} % against the wire in Weir's own steady window (limit ${band} %)`, {
          suspect: ok ? null : anySaturated ? 'judge' : 'weir-math',
          weir: stat(wire.weir), truth: stat(wire.other), error: stat(wireErrors),
        }));
    }

    const truthPairs = pairs(repeats, speed, (repeat) => {
      const truth = truthOf(repeat);
      return usableTruth(truth) ? (dir === 'download' ? truth.downMbps : truth.upMbps) : null;
    });
    const capErrors = errors(truthPairs, 'pct');
    const capMetric: Metric = `${dir}.capacity`;
    const truthValues = truthPairs.other.filter((value): value is number => typeof value === 'number');
    const judgeProblems = repeats.filter((repeat) => truthOf(repeat) && !usableTruth(truthOf(repeat))).length;
    const farMismatch = repeats.filter((repeat) => {
      const far = weirOf(repeat)?.far;
      return typeof far === 'boolean' && far !== expectedFar(shape);
    }).length;
    if (farMismatch) {
      out.push(verdict(id, capMetric, 'invalid-input',
        `Weir's far-link decision differed from the stream count iperf3 used in ${farMismatch} runs`, { suspect: 'input', truth: stat(truthValues) }));
    } else if (capErrors.length < 2) {
      out.push(verdict(id, capMetric, capErrors.length || !failures.length ? 'inconclusive' : 'fail',
        judgeProblems ? `${judgeProblems} iperf3 runs were saturated or disagreed with their own totals` : 'too few runs with both Weir and iperf3',
        { suspect: judgeProblems ? 'judge' : validitySuspect ?? 'unclear', truth: stat(truthValues) }));
    } else {
      const summary = stat(capErrors)!;
      const worst = Math.max(...capErrors.map(Math.abs));
      let ok: boolean;
      let rule: string;
      let halfBand: number;
      if (noisy) {
        const limits = THRESHOLDS.capacityNoisy;
        ok = Math.abs(summary.median) <= limits.medianAbsPct && worst <= limits.worstAbsPct;
        rule = `median within ±${limits.medianAbsPct} %, worst within ${limits.worstAbsPct} %`;
        halfBand = limits.medianAbsPct;
      } else {
        const limits = THRESHOLDS.capacityClean;
        ok = summary.median >= limits.medianMinPct && summary.median <= limits.medianMaxPct && worst <= limits.worstAbsPct;
        rule = `median ${limits.medianMinPct} % to +${limits.medianMaxPct} %, worst within ${limits.worstAbsPct} %`;
        halfBand = (limits.medianMaxPct - limits.medianMinPct) / 2;
      }
      const truthNoise = noise(truthValues) * 100;
      const inconclusive = truthNoise > halfBand * (THRESHOLDS.noise.fractionOfBand * 2);
      let suspect: Suspect | null = null;
      if (!ok) {
        const wireVerdict = out.find((v) => v.metric === metric);
        const wireVsTruth = errors(pairs(repeats, (w) => (dir === 'download' ? w.wire.downMbps : w.wire.upMbps), (repeat) => {
          const truth = truthOf(repeat);
          return usableTruth(truth) ? (dir === 'download' ? truth.downMbps : truth.upMbps) : null;
        }), 'pct');
        const wireMedian = stat(wireVsTruth)?.median ?? 0;
        if (wireVerdict?.status === 'fail') suspect = 'weir-math';
        else if (Math.abs(wireMedian) >= Math.abs(summary.median) - THRESHOLDS.downloadWire.maxAbsPct) suspect = 'weir-load';
        else suspect = 'unclear';
      }
      out.push(verdict(id, capMetric, inconclusive && !ok ? 'inconclusive' : ok ? 'pass' : 'fail',
        `median ${fmt(summary.median)} %, worst ${fmt(worst)} % against iperf3 (${rule}); truth noise ${fmt(truthNoise)} %`,
        { suspect: inconclusive && !ok ? 'judge' : suspect, weir: stat(truthPairs.weir), truth: stat(truthValues), error: summary }));
    }

    const ceiling = ceilingMbps(dir === 'download' ? shape.down : shape.up) * THRESHOLDS.physics.factor;
    const speeds = repeats.map((repeat) => weirOf(repeat)).map((w) => (w ? speed(w) : null)).filter((value): value is number => typeof value === 'number');
    const over = speeds.filter((value) => value > ceiling);
    out.push(verdict(id, `${dir}.physics`, speeds.length ? (over.length ? 'fail' : 'pass') : 'fail',
      speeds.length ? `highest ${fmt(Math.max(...speeds))} Mbps; the link can carry ${fmt(ceiling)} Mbps` : 'no Weir result',
      { suspect: over.length ? 'weir-math' : speeds.length ? null : validitySuspect ?? 'unclear', weir: stat(speeds) }));
  }

  const idleLimits = shape.jitter ? THRESHOLDS.idleJitter : THRESHOLDS.idle;
  const idle = pairs(repeats, (w) => w.report?.latency?.medianMs, (repeat, w) => truthOf(repeat)?.idleMs ?? w.wire.idleMs);
  const idleErrors = errors(idle, 'ms');
  const idleOk = idleErrors.length > 0 && idle.weir.every((weir, index) => {
    const truth = idle.other[index];
    if (typeof weir !== 'number' || typeof truth !== 'number') return true;
    return Math.abs(weir - truth) <= Math.max(idleLimits.absMs, (truth * idleLimits.pct) / 100);
  });
  out.push(verdict(id, 'idle', idleErrors.length ? (idleOk ? 'pass' : 'fail') : 'fail',
    idleErrors.length ? `worst ${fmt(Math.max(...idleErrors.map(Math.abs)))} ms from ping (limit max(${idleLimits.absMs} ms, ${idleLimits.pct} %))` : 'no idle latency to compare',
    { suspect: idleOk ? null : idleErrors.length ? 'weir-math' : validitySuspect ?? 'unclear', weir: stat(idle.weir), truth: stat(idle.other), error: stat(idleErrors) }));

  for (const dir of ['download', 'upload'] as const) {
    const loaded = pairs(repeats, (w) => (dir === 'download' ? w.report?.download.loadedMs : w.report?.upload?.loadedMs),
      (_r, w) => (dir === 'download' ? w.wire.loadedDownMs : w.wire.loadedUpMs));
    const loadedErrors = errors(loaded, 'ms');
    const limits = THRESHOLDS.loaded;
    const ok = loadedErrors.length > 0 && loaded.weir.every((weir, index) => {
      const truth = loaded.other[index];
      if (typeof weir !== 'number' || typeof truth !== 'number') return true;
      return Math.abs(weir - truth) <= Math.max(limits.absMs, (truth * limits.pct) / 100);
    });
    out.push(verdict(id, `loaded.${dir}`, ok ? 'pass' : 'fail',
      loadedErrors.length ? `worst ${fmt(Math.max(...loadedErrors.map(Math.abs)))} ms from a concurrent ping in Weir's window (limit max(${limits.absMs} ms, ${limits.pct} %))` : 'no loaded latency to compare',
      { suspect: ok ? null : loadedErrors.length ? 'weir-math' : validitySuspect ?? 'unclear', weir: stat(loaded.weir), truth: stat(loaded.other), error: stat(loadedErrors) }));
  }

  const profileDef = PROFILES.find((item) => item.id === id);
  if (profileDef?.pair?.role === 'bloat') {
    const partner = all.find((item) => item.id === profileDef.pair!.with);
    const rise = (item: ProfileResult, source: 'weir' | 'ping') => median(item.repeats.flatMap((repeat) => {
      const w = weirOf(repeat);
      const report = w?.report;
      if (!w || !report) return [];
      const idleMs = source === 'weir' ? report.latency?.medianMs : truthOf(repeat)?.idleMs ?? w.wire.idleMs;
      const loadedValues = source === 'weir'
        ? [report.download.loadedMs, report.upload?.loadedMs ?? null]
        : [w.wire.loadedDownMs, w.wire.loadedUpMs];
      const loadedMs = Math.max(...loadedValues.filter((value): value is number => typeof value === 'number'));
      return typeof idleMs === 'number' && Number.isFinite(loadedMs) ? [loadedMs - idleMs] : [];
    }));
    const weirRise = rise(entry, 'weir');
    const pingRise = rise(entry, 'ping');
    const shallowRise = partner ? rise(partner, 'weir') : null;
    const limits = THRESHOLDS.bloat;
    const ok = weirRise !== null && pingRise !== null && shallowRise !== null
      && weirRise >= limits.fraction * pingRise && weirRise >= shallowRise + limits.marginMs;
    out.push(verdict(id, 'bloat', ok ? 'pass' : 'fail',
      `Weir's latency rises ${fmt(weirRise, 0)} ms under load (concurrent ping ${fmt(pingRise, 0)} ms; shallow buffer ${fmt(shallowRise, 0)} ms)`,
      { suspect: ok ? null : 'weir-math' }));
  }

  const bytes = repeats.map((repeat) => weirOf(repeat)?.wire.ipBytes).filter((value): value is number => typeof value === 'number');
  const ceilingBytes = THRESHOLDS.dataCeiling.mb * 1e6;
  const overBytes = bytes.filter((value) => value > ceilingBytes);
  out.push(verdict(id, 'data.ceiling', bytes.length ? (overBytes.length ? 'fail' : 'pass') : 'fail',
    bytes.length ? `most ${fmt(Math.max(...bytes) / 1e6, 0)} MB on the wire (promise: up to ${THRESHOLDS.dataCeiling.mb} MB)` : 'no wire count',
    { suspect: overBytes.length ? 'weir-math' : bytes.length ? null : 'judge', weir: stat(bytes.map((value) => value / 1e6)) }));

  const honesty = repeats.map((repeat) => {
    const w = weirOf(repeat);
    return w?.report && w.wire.payloadBytes ? w.report.bytesUsed / w.wire.payloadBytes : null;
  });
  out.push(verdict(id, 'data.honesty', 'report', `Weir counts ${fmt((stat(honesty)?.median ?? NaN) * 100, 0)} % of the TCP payload that crossed the wire`, { weir: stat(honesty) }));

  const jitterPairs = pairs(repeats, (w) => w.report?.latency?.jitterMs, (repeat) => truthOf(repeat)?.jitterMs);
  out.push(verdict(id, 'jitter', 'report', `Weir ${fmt(stat(jitterPairs.weir)?.median, 2)} ms, ping ${fmt(stat(jitterPairs.other)?.median, 2)} ms`, {
    weir: stat(jitterPairs.weir), truth: stat(jitterPairs.other),
  }));

  const iperfLoaded = pairs(repeats, (w) => w.report?.download.loadedMs, (repeat) => truthOf(repeat)?.loadedDownMs);
  out.push(verdict(id, 'loaded.iperf', 'report', `Weir ${fmt(stat(iperfLoaded.weir)?.median)} ms under its own load; ping ${fmt(stat(iperfLoaded.other)?.median)} ms under iperf3`, {
    weir: stat(iperfLoaded.weir), truth: stat(iperfLoaded.other),
  }));

  const twins = repeats.map(twinOf).filter((twin): twin is Twin => !!twin && !twin.errors.length && twin.blocked === 0);
  const twinBroken = repeats.filter((repeat) => repeat.twin && (failed(repeat.twin) || twinOf(repeat)!.errors.length > 0 || twinOf(repeat)!.blocked > 0)).length;
  const ceilingNote = twinCeilingMbps !== null && Math.max(shape.down, shape.up) > 0.8 * twinCeilingMbps ? '; above 80 % of the twin\'s own ceiling on this runner' : '';
  const brokenNote = twinBroken ? `; ${twinBroken} twin runs failed or were blocked` : '';
  const weirMedian = (pick: (w: Weir) => number | null | undefined) => stat(repeats.map((repeat) => {
    const w = weirOf(repeat);
    return w ? pick(w) : null;
  }));
  for (const [metric, pick, weirPick, unit] of [
    ['twin.download', (twin: Twin) => twin.down, (w: Weir) => w.report?.download.mbps, 'Mbps'],
    ['twin.upload', (twin: Twin) => twin.up, (w: Weir) => w.report?.upload?.mbps, 'Mbps'],
    ['twin.idle', (twin: Twin) => twin.idle, (w: Weir) => w.report?.latency?.medianMs, 'ms'],
  ] as const) {
    const twinStat = stat(twins.map(pick));
    const weirStat = weirMedian(weirPick);
    const diff = twinStat && weirStat ? (unit === 'ms' ? weirStat.median - twinStat.median : pct(weirStat.median, twinStat.median)) : null;
    out.push(verdict(id, metric, 'report',
      twinStat ? `twin ${fmt(twinStat.median)} ${unit}, Weir ${fmt(weirStat?.median)} ${unit} (${unit === 'ms' ? `${fmt(diff)} ms` : `${fmt(diff)} %`})${ceilingNote}${brokenNote}` : `no twin result${brokenNote}`,
      { weir: weirStat, truth: twinStat, error: diff === null ? null : { median: diff, min: diff, max: diff, n: 1 } }));
  }

  if (shape.uploadTimingIncludesBody) {
    const reference = all.find((item) => item.id === 'cable-100-10-cf');
    const mine = stat(repeats.map((repeat) => weirOf(repeat)?.report?.upload?.mbps));
    const theirs = reference ? stat(reference.repeats.map((repeat) => weirOf(repeat)?.report?.upload?.mbps)) : null;
    const diff = mine && theirs ? pct(mine.median, theirs.median) : null;
    out.push(verdict(id, 'qreal.upload', 'report',
      `Weir's upload reads ${fmt(diff)} % against the same link when Server-Timing excludes the body`, { weir: mine, truth: theirs }));
  }

  return out;
}

function gatedStatus(verdicts: Verdict[], reportOnly: boolean, validity: Validity): Verdict[] {
  return verdicts.map((item) => {
    if (REPORT_ONLY.has(item.metric) || item.status === 'report') return { ...item, status: 'report' };
    if (reportOnly) return { ...item, status: 'report', detail: `${item.detail} (report only)` };
    if (!validity.ok) return { ...item, status: 'invalid-input', suspect: 'input', detail: `${item.detail}; lab input invalid: ${validity.reasons.join('; ')}` };
    return item;
  });
}

function baselineVerdicts(shard: number, baseline: Baseline | null): Verdict[] {
  const id = `baseline-${shard}`;
  if (!baseline) return [verdict(id, 'baseline', 'fail', 'no baseline recorded', { suspect: 'judge' })];
  const limits = THRESHOLDS.baseline;
  const perGbps = baseline.serverCpuCores !== null && baseline.serverGbps ? baseline.serverCpuCores / baseline.serverGbps : null;
  const checks: Array<[string, boolean, string, Suspect]> = [
    ['iperf3', (baseline.iperfDown ?? 0) >= limits.minMbps && (baseline.iperfUp ?? 0) >= limits.minMbps,
      `iperf3 ${fmt(baseline.iperfDown, 0)}/${fmt(baseline.iperfUp, 0)} Mbps unshaped (need ${limits.minMbps})`, 'judge'],
    ['weir', (baseline.weirDown ?? 0) >= limits.minMbps && (baseline.weirUp ?? 0) >= limits.minMbps,
      `Weir ${fmt(baseline.weirDown, 0)}/${fmt(baseline.weirUp, 0)} Mbps unshaped (need ${limits.minMbps})${baseline.weirError ? `, ${baseline.weirError}` : ''}`, 'weir-load'],
    ['server', perGbps !== null && perGbps < limits.maxServerCoresPerGbps,
      `lab server used ${fmt(baseline.serverCpuCores, 2)} cores at ${fmt(baseline.serverGbps, 2)} Gbps, ${fmt(perGbps, 2)} cores per Gbps (limit ${limits.maxServerCoresPerGbps})`, 'judge'],
    ['softnet', baseline.softnetDrops === 0, `${baseline.softnetDrops} packets dropped before the shaper`, 'input'],
    ['inflation', baseline.weirLoadedInflationMs !== null && Math.abs(baseline.weirLoadedInflationMs) <= limits.maxInflationMs,
      `Weir's loaded latency ${fmt(baseline.weirLoadedInflationMs, 2)} ms above a concurrent ping, unshaped (limit ${limits.maxInflationMs} ms)`, 'weir-math'],
  ];
  return checks.map(([name, ok, detail, suspect]) => verdict(`${id}.${name}`, 'baseline', ok ? 'pass' : 'fail', detail, { suspect: ok ? null : suspect }));
}

export function applyFindings(verdicts: Verdict[], findings: Finding[]): Verdict[] {
  return verdicts.map((item) => {
    const covering = findings.find((finding) => finding.status === 'open'
      && finding.expect.some((pair) => pair.profile === item.profile && pair.metric === item.metric));
    if (!covering) return item;
    if (item.status === 'fail') return { ...item, status: 'xfail', finding: covering.id };
    if (item.status === 'pass') return { ...item, status: 'xpass', finding: covering.id };
    return { ...item, finding: covering.id };
  });
}

function twinCeiling(results: Results): number | null {
  const values = results.shards.flatMap((shard) => [shard.baseline?.twinDown, shard.baseline?.twinUp]);
  return stat(values)?.min ?? null;
}

export function analyze(results: Results, findings: Finding[], expected: Profile[] = PROFILES): Analysis {
  const verdicts: Verdict[] = [];
  for (const shard of [...new Set(expected.map((entry) => entry.shard))].sort((a, b) => a - b)) {
    const found = results.shards.find((item) => item.shard === shard);
    verdicts.push(...(found ? baselineVerdicts(shard, found.baseline) : [verdict(`baseline-${shard}`, 'coverage', 'fail', `shard ${shard} missing`, { suspect: 'judge' })]));
  }
  const ceiling = twinCeiling(results);
  for (const definition of expected) {
    const entry = results.profiles.find((item) => item.id === definition.id);
    if (!entry) {
      verdicts.push(verdict(definition.id, 'coverage', 'fail', 'profile missing from the results', { suspect: 'judge' }));
      continue;
    }
    if (entry.repeats.length < definition.repeats) {
      verdicts.push(verdict(definition.id, 'coverage', 'fail', `${entry.repeats.length} of ${definition.repeats} repeats`, { suspect: 'judge' }));
    }
    verdicts.push(...gatedStatus(profileVerdicts(entry, results.profiles, ceiling), definition.reportOnly, entry.validity));
  }
  const final = applyFindings(verdicts, findings);
  const counts = { pass: 0, fail: 0, xfail: 0, xpass: 0, inconclusive: 0, 'invalid-input': 0, report: 0 } as Record<Status, number>;
  for (const item of final) counts[item.status] += 1;
  return {
    verdicts: final,
    ok: final.every((item) => GATED_OK.has(item.status)),
    counts,
    facts: facts(results, final),
  };
}

/** Values findings prose may cite as {profile.key}. */
export function facts(results: Results, verdicts: Verdict[]): Record<string, Record<string, string>> {
  const table: Record<string, Record<string, string>> = {};
  for (const entry of results.profiles) {
    const row: Record<string, string> = {};
    const put = (key: string, value: number | null | undefined, digits = 1) => {
      row[key] = fmt(value, digits);
    };
    const weirs = entry.repeats.map(weirOf);
    const truths = entry.repeats.map(truthOf);
    for (const dir of ['download', 'upload'] as const) {
      const speeds = weirs.map((w) => (dir === 'download' ? w?.report?.download.mbps : w?.report?.upload?.mbps));
      put(`${dir}.weir`, stat(speeds)?.median);
      put(`${dir}.wire`, stat(weirs.map((w) => (dir === 'download' ? w?.wire.downMbps : w?.wire.upMbps)))?.median);
      put(`${dir}.truth`, stat(truths.map((t) => (dir === 'download' ? t?.downMbps : t?.upMbps)))?.median);
      const capacity = verdicts.find((item) => item.profile === entry.id && item.metric === `${dir}.capacity`);
      put(`${dir}.err`, capacity?.error?.median);
      const wire = verdicts.find((item) => item.profile === entry.id && item.metric === `${dir}.wire`);
      put(`${dir}.wireErr`, wire?.error?.median);
      put(`${dir}.loaded.weir`, stat(weirs.map((w) => (dir === 'download' ? w?.report?.download.loadedMs : w?.report?.upload?.loadedMs)))?.median, 0);
      put(`${dir}.loaded.ping`, stat(weirs.map((w) => (dir === 'download' ? w?.wire.loadedDownMs : w?.wire.loadedUpMs)))?.median, 0);
      row[`${dir}.rough`] = String(weirs.filter((w) => (dir === 'download' ? w?.report?.download.rough : w?.report?.upload?.rough)).length);
      row[`${dir}.capped`] = String(weirs.filter((w) => (dir === 'download' ? w?.report?.download.capped : w?.report?.upload?.capped)).length);
    }
    put('idle.weir', stat(weirs.map((w) => w?.report?.latency?.medianMs))?.median);
    put('idle.truth', stat(truths.map((t) => t?.idleMs))?.median);
    put('data.mb', stat(weirs.map((w) => (w?.wire.ipBytes ?? NaN) / 1e6))?.max, 0);
    put('bytesUsed.mb', stat(weirs.map((w) => (w?.report?.bytesUsed ?? NaN) / 1e6))?.median, 0);
    put('honesty.pct', (stat(weirs.map((w) => (w?.report && w.wire.payloadBytes ? w.report.bytesUsed / w.wire.payloadBytes : null)))?.median ?? NaN) * 100, 0);
    row.runs = String(entry.repeats.length);
    row.errors = countWords(weirFailures(entry)) || 'none';
    table[entry.id] = row;
  }
  return table;
}

const PLACEHOLDER = /\{([a-z0-9.-]+?)\.([a-zA-Z.]+)\}/g;

/** Fills {profile.key} placeholders; unknown ones are left visible as {?…}. */
export function fill(text: string, table: Record<string, Record<string, string>>): string {
  return text.replace(PLACEHOLDER, (_whole, profileId: string, key: string) => table[profileId]?.[key] ?? `{?${profileId}.${key}}`);
}

export function placeholders(text: string): Array<{ profile: string; key: string }> {
  return [...text.matchAll(PLACEHOLDER)].map((match) => ({ profile: match[1]!, key: match[2]! }));
}

/** Lab input checks for one profile, from the measurements run.ts took. */
export function validity(
  entry: Profile,
  delaySamples: number[],
  truths: Truth[],
  loss: { down: number | null; up: number | null },
  softnet: number,
  txDropped: number,
  reorder: number,
): Validity {
  const reasons: string[] = [];
  const round = (value: number | null, digits = 3) => (value === null ? null : Number(value.toFixed(digits)));
  const medianMs = median(delaySamples);
  const p5 = percentile(delaySamples, 0.05);
  const p95 = percentile(delaySamples, 0.95);
  let delayOk: boolean;
  if (entry.jitter) {
    const limits = THRESHOLDS.jitterDelay;
    const bound = entry.rttMs + limits.p95Spreads * entry.jitter.ms;
    const centre = Math.max(limits.medianAbsMs, (entry.rttMs * limits.medianPct) / 100);
    delayOk = medianMs !== null && p5 !== null && p95 !== null && Math.abs(medianMs - entry.rttMs) <= centre
      && p95 <= bound && p95 - p5 >= limits.minSpreadMs;
    if (!delayOk) reasons.push(`delay: median ${fmt(medianMs, 2)} ms, p5 ${fmt(p5, 2)} ms, p95 ${fmt(p95, 2)} ms (want median within ${fmt(centre)} ms of ${entry.rttMs}, p95 ≤ ${fmt(bound)} ms, spread ≥ ${limits.minSpreadMs} ms)`);
  } else {
    const tolerance = Math.max(THRESHOLDS.delay.absMs, (entry.rttMs * THRESHOLDS.delay.pct) / 100);
    delayOk = medianMs !== null && Math.abs(medianMs - entry.rttMs) <= tolerance;
    if (!delayOk) reasons.push(`delay: median ${fmt(medianMs, 2)} ms for ${entry.rttMs} ms`);
  }
  let rate: Validity['rate'] = null;
  if (entry.lossPct === 0 && !entry.jitter && truths.length) {
    const ratios = truths.flatMap((truth) => [truth.downMbps / ceilingMbps(entry.down), truth.upMbps / ceilingMbps(entry.up)]);
    const ok = ratios.every((ratio) => ratio >= THRESHOLDS.rate.min && ratio <= THRESHOLDS.rate.max);
    rate = { ok, ratios: ratios.map((ratio) => round(ratio, 4)!), detail: 'loss-free iperf3 goodput over the shaped payload ceiling' };
    if (!ok) reasons.push(`rate: iperf3/ceiling ${ratios.map((ratio) => ratio.toFixed(3)).join(', ')}`);
  }
  let lossCheck: Validity['loss'] = null;
  if (entry.lossPct > 0) {
    const target = entry.lossPct / 100;
    const band = entry.lossPct < 0.5 ? 0.5 : 0.3;
    const near = (value: number | null) => value !== null && Math.abs(value - target) <= target * band;
    const ok = near(loss.down) && near(loss.up);
    lossCheck = { ok, down: round(loss.down, 5), up: round(loss.up, 5), detail: `netem drops within ±${band * 100} % of ${entry.lossPct} % each way` };
    if (!ok) reasons.push(`loss: ${fmt((loss.down ?? NaN) * 100, 3)} % down, ${fmt((loss.up ?? NaN) * 100, 3)} % up for ${entry.lossPct} %`);
  }
  const dropsOk = softnet === 0 && txDropped === 0;
  if (!dropsOk) reasons.push(`drops: softnet ${softnet}, tx_dropped ${txDropped}`);
  let bloat: Validity['bloat'] = null;
  if (entry.pair && truths.length) {
    const loaded = median(truths.flatMap((truth) => [truth.loadedDownMs, truth.loadedUpMs]).filter((value): value is number => value !== null));
    const ok = loaded !== null && (entry.pair.role === 'bloat'
      ? loaded >= entry.rttMs + THRESHOLDS.bloatInput.deepFraction * entry.bufferMs
      : loaded <= entry.rttMs + entry.bufferMs + THRESHOLDS.bloatInput.shallowSlackMs);
    bloat = { ok, loadedMs: round(loaded, 1), detail: entry.pair.role === 'bloat' ? 'iperf3 fills the deep buffer' : 'the shallow buffer stays shallow' };
    if (!ok) reasons.push(`bloat: iperf3-loaded ${fmt(loaded)} ms`);
  }
  const reorderCheck: Validity['reorder'] = { ok: true, events: reorder, detail: 'recorded, not gated: TCP also counts spurious-retransmit detections after tail drops' };
  return {
    delay: { ok: delayOk, medianMs: round(medianMs), p5: round(p5), p95: round(p95), detail: 'idle ping against the configured delay' },
    rate,
    loss: lossCheck,
    drops: { ok: dropsOk, softnet, txDropped, detail: 'no packets dropped before the shaper' },
    bloat,
    reorder: reorderCheck,
    ok: reasons.length === 0,
    reasons,
  };
}
