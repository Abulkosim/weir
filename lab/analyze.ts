// Pure judging: lab results in, verdicts out. No I/O and no clocks.

import { median, percentile } from './parse.ts';
import { ceilingMbps, expectedFar, PROFILES, type Profile } from './profiles.ts';
import { failed, type Baseline, type ProfileResult, type Repeat, type Results, type Truth, type Twin, type Validity, type Weir } from './schema.ts';
import { THRESHOLDS } from './thresholds.ts';

export type Status = 'pass' | 'fail' | 'xfail' | 'xpass' | 'inconclusive' | 'invalid-input' | 'report';
export type Suspect = 'input' | 'judge' | 'weir-math' | 'weir-load' | 'unclear';

/** Never fails the gate on its own. inconclusive and invalid-input warn up to THRESHOLDS.unjudged; fail always fails. */
export const GATED_OK: ReadonlySet<Status> = new Set(['pass', 'xfail', 'xpass', 'report']);
export const UNJUDGED: ReadonlySet<Status> = new Set(['inconclusive', 'invalid-input']);

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
  'loaded.download.bias',
  'loaded.upload.bias',
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
  // Report only until the lab records ping losses: see thresholds.loadedBias.
  'loaded.download.bias', 'loaded.upload.bias',
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
  /** validity only: each distinct way a run failed, e.g. "upload stalled". */
  kinds?: string[];
  /** Finished runs in which Weir gave no value, and what the check said about the rest. */
  gaps?: { count: number; status: Status };
}

/**
 * One check a finding expects to fail, and how. A failure outside the bound is a new failure and fails
 * the gate. One kind of bound: `errors` (validity: every way a run failed is one of these), `missing`
 * (the check fails only for want of any Weir result), or `min`/`max` (every per-repeat error in range).
 * `gaps` allows up to that many finished runs without Weir's value; the rest must pass, or stay in
 * `min`/`max` when given.
 */
export interface Expectation {
  profile: string;
  metric: Metric;
  errors?: string[];
  missing?: boolean;
  gaps?: number;
  min?: number;
  max?: number;
  /** How often this check failed in the runs the bound was set from. */
  seen: { failed: number; runs: number };
}

export interface Finding {
  id: string;
  title: string;
  status: 'open' | 'fixed';
  where: string;
  cause: string;
  expect: Expectation[];
}

export interface Gate {
  ok: boolean;
  errors: string[];
  warnings: string[];
}

export interface Analysis {
  verdicts: Verdict[];
  ok: boolean;
  gate: Gate;
  counts: Record<Status, number>;
  facts: Record<string, Record<string, string>>;
}

export function stat(values: Array<number | null | undefined>): Stat | null {
  const clean = values.filter((value): value is number => typeof value === 'number' && Number.isFinite(value));
  if (!clean.length) return null;
  return { median: median(clean)!, min: Math.min(...clean), max: Math.max(...clean), n: clean.length };
}

const weirOf = (repeat: Repeat): Weir | null => (failed(repeat.weir) ? null : repeat.weir);
/** The ping taken during Weir's own latency phase; the truth step's ping, taken at another time, only when that is missing. */
const idlePing = (repeat: Repeat, w: Weir): number | null | undefined => w.wire.idleMs ?? truthOf(repeat)?.idleMs;
const truthOf = (repeat: Repeat): Truth | null => (repeat.truth && !failed(repeat.truth) ? repeat.truth : null);
const twinOf = (repeat: Repeat): Twin | null => (repeat.twin && !failed(repeat.twin) ? repeat.twin : null);

/** Truth a judge can stand on: not saturated and iperf3's own sums agree for that direction. */
export function usableTruth(truth: Truth | null, dir: 'download' | 'upload'): truth is Truth {
  return consistentTruth(truth, dir);
}

/** A usable truth that, on a loss-free link, also filled it: an iperf3 run that under-fills a clean link is not bulk TCP's capacity. */
export function capacityTruth(truth: Truth | null, dir: 'download' | 'upload', shape: Profile | ProfileResult['shape']): truth is Truth {
  if (!consistentTruth(truth, dir)) return false;
  if (isNoisy(shape)) return true;
  const ratio = dir === 'download' ? truth.downMbps / ceilingMbps(shape.down) : truth.upMbps / ceilingMbps(shape.up);
  return ratio >= THRESHOLDS.rate.min;
}

function consistentTruth(truth: Truth | null, dir: 'download' | 'upload'): truth is Truth {
  return !!truth && !truth.saturated
    && (dir === 'download' ? truth.mismatch.down : truth.mismatch.up) <= THRESHOLDS.iperfConsistency.maxPct / 100;
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

interface Gaps {
  /** Finished runs for this check. */
  runs: number;
  /** Finished runs in which Weir gave no value. */
  weir: number;
  /** Finished runs in which the reference had no value. */
  other: number;
}

/** Counts finished runs that the paired judges would otherwise skip without a trace. */
function gaps(repeats: Repeat[], done: (w: Weir) => boolean, weir: (w: Weir) => number | null | undefined, other: (r: Repeat, w: Weir) => number | null | undefined): Gaps {
  const out: Gaps = { runs: 0, weir: 0, other: 0 };
  for (const repeat of repeats) {
    const w = weirOf(repeat);
    if (!w || !done(w)) continue;
    out.runs += 1;
    if (typeof weir(w) !== 'number') out.weir += 1;
    else if (typeof other(repeat, w) !== 'number') out.other += 1;
  }
  return out;
}

/** A finished run without Weir's value fails the check; more than one without a reference leaves it unjudged. */
function covered(item: Verdict, found: Gaps, what: string): Verdict {
  if (found.weir > 0) {
    return {
      ...item, status: 'fail', suspect: 'weir-math', gaps: { count: found.weir, status: item.status },
      detail: `${item.detail}; Weir gave no ${what} in ${found.weir} of ${found.runs} finished runs`,
    };
  }
  if (found.other > 1 && item.status === 'pass') {
    return { ...item, status: 'inconclusive', suspect: 'judge', detail: `${item.detail}; no reference in ${found.other} of ${found.runs} finished runs` };
  }
  return item;
}

const finishedDown = (w: Weir) => !!w.report;
const finishedUp = (w: Weir) => !!w.report?.upload;

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
          kinds: [...new Set(failures)].sort(),
        })
      : verdict(id, 'validity', 'pass', `${repeats.length} of ${repeats.length} runs finished with a download and an upload`),
  );
  const validitySuspect = out[0]!.suspect;

  for (const dir of ['download', 'upload'] as const) {
    const band = dir === 'download' ? THRESHOLDS.downloadWire.maxAbsPct : THRESHOLDS.uploadWire.maxAbsPct;
    const speed = (w: Weir) => (dir === 'download' ? w.report?.download.mbps : w.report?.upload?.mbps);
    const finished = dir === 'download' ? finishedDown : finishedUp;
    const wireOf = (_r: Repeat, w: Weir) => (dir === 'download' ? w.wire.downMbps : w.wire.upMbps);
    const wire = pairs(repeats, speed, wireOf);
    const wireErrors = errors(wire, 'pct');
    const metric: Metric = `${dir}.wire`;
    if (!wireErrors.length) {
      out.push(verdict(id, metric, 'fail', 'no Weir result to compare with the wire', { suspect: validitySuspect ?? 'unclear' }));
    } else {
      const worst = Math.max(...wireErrors.map(Math.abs));
      const ok = worst <= band;
      out.push(covered(verdict(id, metric, anySaturated && !ok ? 'inconclusive' : ok ? 'pass' : 'fail',
        `worst ${fmt(worst)} % against the wire in Weir's own steady window (limit ${band} %)`, {
          suspect: ok ? null : anySaturated ? 'judge' : 'weir-math',
          weir: stat(wire.weir), truth: stat(wire.other), error: stat(wireErrors),
        }), gaps(repeats, finished, speed, wireOf), `${dir} speed`));
    }

    const truthPairs = pairs(repeats, speed, (repeat) => {
      const truth = truthOf(repeat);
      return capacityTruth(truth, dir, shape) ? (dir === 'download' ? truth.downMbps : truth.upMbps) : null;
    });
    const capErrors = errors(truthPairs, 'pct');
    const capMetric: Metric = `${dir}.capacity`;
    const truthValues = truthPairs.other.filter((value): value is number => typeof value === 'number');
    const judgeProblems = repeats.filter((repeat) => truthOf(repeat) && !capacityTruth(truthOf(repeat), dir, shape)).length;
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
          return capacityTruth(truth, dir, shape) ? (dir === 'download' ? truth.downMbps : truth.upMbps) : null;
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

    const physical = ceilingMbps(dir === 'download' ? shape.down : shape.up);
    const ceiling = physical * THRESHOLDS.physics.factor;
    const speeds = repeats.map((repeat) => weirOf(repeat)).map((w) => (w ? speed(w) : null)).filter((value): value is number => typeof value === 'number');
    const over = speeds.filter((value) => value > ceiling);
    out.push(verdict(id, `${dir}.physics`, speeds.length ? (over.length ? 'fail' : 'pass') : 'fail',
      speeds.length ? `highest ${fmt(Math.max(...speeds))} Mbps; the link can carry ${fmt(ceiling)} Mbps` : 'no Weir result',
      {
        suspect: over.length ? 'weir-math' : speeds.length ? null : validitySuspect ?? 'unclear',
        weir: stat(speeds), error: stat(speeds.map((value) => pct(value, physical))),
      }));
  }

  const idleLimits = shape.jitter ? THRESHOLDS.idleJitter : THRESHOLDS.idle;
  const idleWeirOf = (w: Weir) => w.report?.latency?.medianMs;
  const idle = pairs(repeats, idleWeirOf, idlePing);
  const idleErrors = errors(idle, 'ms');
  const within = (weir: number, truth: number) => Math.abs(weir - truth) <= Math.max(idleLimits.absMs, (truth * idleLimits.pct) / 100);
  const idleWeir = stat(idle.weir);
  const idleTruth = stat(idle.other);
  const idleOk = idleErrors.length > 0 && idle.weir.every((weir, index) => {
    const truth = idle.other[index];
    return typeof weir !== 'number' || typeof truth !== 'number' || within(weir, truth);
  });
  out.push(covered(verdict(id, 'idle', idleErrors.length ? (idleOk ? 'pass' : 'fail') : 'fail',
    idleErrors.length
      ? `medians ${fmt(idleWeir?.median)} ms Weir, ${fmt(idleTruth?.median)} ms ping in Weir's latency phase; worst repeat ${fmt(Math.max(...idleErrors.map(Math.abs)))} ms apart (limit max(${idleLimits.absMs} ms, ${idleLimits.pct} %), every repeat)`
      : 'no idle latency to compare',
    { suspect: idleOk ? null : idleErrors.length ? 'weir-math' : validitySuspect ?? 'unclear', weir: stat(idle.weir), truth: stat(idle.other), error: stat(idleErrors) }),
  gaps(repeats, finishedDown, idleWeirOf, idlePing), 'idle latency'));

  for (const dir of ['download', 'upload'] as const) {
    const loadedWeir = (w: Weir) => (dir === 'download' ? w.report?.download.loadedMs : w.report?.upload?.loadedMs);
    const loadedRef = (_r: Repeat, w: Weir) => (dir === 'download' ? w.wire.loadedDownMs : w.wire.loadedUpMs);
    const loaded = pairs(repeats, loadedWeir, loadedRef);
    const loadedErrors = errors(loaded, 'ms');
    const limits = THRESHOLDS.loaded;
    const ok = loadedErrors.length > 0 && loaded.weir.every((weir, index) => {
      const truth = loaded.other[index];
      if (typeof weir !== 'number' || typeof truth !== 'number') return true;
      return Math.abs(weir - truth) <= Math.max(limits.absMs, (truth * limits.pct) / 100);
    });
    const found = gaps(repeats, dir === 'download' ? finishedDown : finishedUp, loadedWeir, loadedRef);
    out.push(covered(verdict(id, `loaded.${dir}`, ok ? 'pass' : 'fail',
      loadedErrors.length ? `worst ${fmt(Math.max(...loadedErrors.map(Math.abs)))} ms from a concurrent ping in Weir's window (limit max(${limits.absMs} ms, ${limits.pct} %))` : 'no loaded latency to compare',
      { suspect: ok ? null : loadedErrors.length ? 'weir-math' : validitySuspect ?? 'unclear', weir: stat(loaded.weir), truth: stat(loaded.other), error: stat(loadedErrors) }),
    found, `${dir} loaded latency`));

    const bias = stat(loadedErrors);
    const reference = stat(loaded.other);
    const biasLimit = reference ? Math.max(THRESHOLDS.loadedBias.absMs, (reference.median * THRESHOLDS.loadedBias.pct) / 100) : null;
    const biasOk = !!bias && biasLimit !== null && Math.abs(bias.median) <= biasLimit;
    out.push(covered(verdict(id, `loaded.${dir}.bias`, biasOk ? 'pass' : 'fail',
      bias && reference
        ? `median signed error ${fmt(bias.median)} ms over ${bias.n} runs against a ${fmt(reference.median, 0)} ms ping (limit ±${fmt(biasLimit)} ms)`
        : 'no loaded latency to compare',
      {
        suspect: biasOk ? null : bias ? 'weir-math' : validitySuspect ?? 'unclear',
        weir: stat(loaded.weir), truth: reference, error: bias ? { median: bias.median, min: bias.median, max: bias.median, n: bias.n } : null,
      }), found, `${dir} loaded latency`));
  }

  const profileDef = PROFILES.find((item) => item.id === id);
  if (profileDef?.pair?.role === 'bloat') {
    const partner = all.find((item) => item.id === profileDef.pair!.with);
    const rise = (item: ProfileResult, source: 'weir' | 'ping') => median(item.repeats.flatMap((repeat) => {
      const w = weirOf(repeat);
      const report = w?.report;
      if (!w || !report) return [];
      const idleMs = source === 'weir' ? report.latency?.medianMs : idlePing(repeat, w);
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
    {
      suspect: overBytes.length ? 'weir-math' : bytes.length ? null : 'judge',
      weir: stat(bytes.map((value) => value / 1e6)), error: stat(bytes.map((value) => pct(value, ceilingBytes))),
    }));

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
      `Weir's upload reads ${fmt(mine?.median)} Mbps when Server-Timing includes the body, ${diff !== null && diff >= 0 ? '+' : ''}${fmt(diff)} % against ${fmt(theirs?.median)} Mbps on the same link when it does not`, { weir: mine, truth: theirs }));
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
  return checks.map(([name, ok, detail, suspect]) => name === 'inflation'
    ? verdict(`${id}.${name}`, 'baseline', 'report', `${detail}; report only: unshaped, Weir saturates the host's CPUs, so this measures CPU contention rather than the server`)
    : verdict(`${id}.${name}`, 'baseline', ok ? 'pass' : 'fail', detail, { suspect: ok ? null : suspect }));
}

/** The expected failure in words, e.g. "every per-repeat error within −12 to +5". */
export function describeBound(expectation: Expectation): string {
  if (expectation.errors) return `only ${expectation.errors.join(', ')}`;
  if (expectation.missing) return 'only for want of any Weir result';
  const edge = (value: number | undefined) => (value === undefined ? 'any' : `${value > 0 ? '+' : ''}${value}`);
  const { min, max } = expectation;
  const range = min !== undefined && max !== undefined ? `every per-repeat error within ${edge(min)} to ${edge(max)}`
    : max !== undefined ? `every per-repeat error at most ${edge(max)}`
      : min !== undefined ? `every per-repeat error at least ${edge(min)}`
        : 'otherwise passing';
  return expectation.gaps ? `up to ${expectation.gaps} runs without Weir's value, ${range}` : range;
}

const inRange = (item: Verdict, expectation: Expectation) => !!item.error
  && (expectation.min === undefined || item.error.min >= expectation.min)
  && (expectation.max === undefined || item.error.max <= expectation.max);

/** Whether a failing verdict fails the way its finding expects. */
export function withinBound(item: Verdict, expectation: Expectation): boolean {
  if (expectation.errors) return !!item.kinds?.length && item.kinds.every((kind) => expectation.errors!.includes(kind));
  if (expectation.missing) return item.weir === null && item.error === null;
  const ranged = expectation.min !== undefined || expectation.max !== undefined;
  if (expectation.gaps) {
    if (!item.gaps) return ranged && inRange(item, expectation);
    if (item.gaps.count > expectation.gaps) return false;
    return item.gaps.status === 'pass' || (ranged && inRange(item, expectation));
  }
  return !item.gaps && ranged && inRange(item, expectation);
}

export function applyFindings(verdicts: Verdict[], findings: Finding[]): Verdict[] {
  return verdicts.map((item) => {
    let expectation: Expectation | undefined;
    const covering = findings.find((finding) => {
      if (finding.status !== 'open') return false;
      expectation = finding.expect.find((pair) => pair.profile === item.profile && pair.metric === item.metric);
      return !!expectation;
    });
    if (!covering || !expectation) return item;
    if (item.status === 'fail') {
      return withinBound(item, expectation)
        ? { ...item, status: 'xfail', finding: covering.id }
        : { ...item, finding: covering.id, detail: `${item.detail}; outside ${covering.id}'s expected failure (${describeBound(expectation)})` };
    }
    if (item.status === 'pass') return { ...item, status: 'xpass', finding: covering.id };
    return { ...item, finding: covering.id };
  });
}

/**
 * The gate. Fails on any fail; on inconclusive or invalid-input over more than THRESHOLDS.unjudged.maxLinks
 * links; and on a check that failed in every run when its bound was set but passed both in this run and
 * in `previous` (the run published in docs/accuracy.json): its finding is fixed or wrong and must be
 * closed or narrowed. A check that failed only in some runs may pass; that warns.
 */
export function gate(verdicts: Verdict[], findings: Finding[], previous: Verdict[] | null = null): Gate {
  const errors: string[] = [];
  const warnings: string[] = [];
  for (const item of verdicts.filter((verdict) => verdict.status === 'fail')) errors.push(`${item.profile} ${item.metric} fail: ${item.detail}`);
  const unjudged = verdicts.filter((verdict) => UNJUDGED.has(verdict.status));
  const links = [...new Set(unjudged.map((item) => item.profile))];
  for (const item of unjudged) warnings.push(`${item.profile} ${item.metric} ${item.status}: ${item.detail}`);
  if (links.length > THRESHOLDS.unjudged.maxLinks) {
    errors.push(`the lab could not judge ${links.length} links (${links.join(', ')}); limit ${THRESHOLDS.unjudged.maxLinks}`);
  }
  const passed = (list: Verdict[], pair: Expectation) =>
    list.some((item) => item.profile === pair.profile && item.metric === pair.metric && item.status === 'xpass');
  for (const finding of findings.filter((item) => item.status === 'open')) {
    for (const pair of finding.expect) {
      if (!passed(verdicts, pair)) continue;
      const steady = pair.seen.failed === pair.seen.runs;
      if (steady && previous && passed(previous, pair)) {
        errors.push(`${finding.id} ${pair.profile} ${pair.metric}: failed in ${pair.seen.failed} of ${pair.seen.runs} runs when its bound was set, but passed in this run and in the published run; close or narrow ${finding.id}`);
      } else {
        warnings.push(`${pair.profile} ${pair.metric} passes but ${finding.id} expects it to fail (it failed in ${pair.seen.failed} of ${pair.seen.runs} runs when set)`);
      }
    }
  }
  return { ok: errors.length === 0, errors, warnings };
}

function twinCeiling(results: Results): number | null {
  const values = results.shards.flatMap((shard) => [shard.baseline?.twinDown, shard.baseline?.twinUp]);
  return stat(values)?.min ?? null;
}

/**
 * The rate input check, from the iperf3 truth stored with each repeat. Recomputed when judging, so a
 * change to the check applies to results already recorded. The best run per direction must reach the
 * lower bound (one run can under-fill a shallow buffer); the median per direction must stay under the
 * upper bound (a leaking shaper leaks every run, iperf3's edge noise does not).
 */
export function rateCheck(entry: Pick<Profile, 'down' | 'up' | 'lossPct' | 'jitter'>, truths: Truth[]): Validity['rate'] {
  if (entry.lossPct !== 0 || entry.jitter || !truths.length) return null;
  const down = truths.filter((truth) => usableTruth(truth, 'download')).map((truth) => truth.downMbps / ceilingMbps(entry.down));
  const up = truths.filter((truth) => usableTruth(truth, 'upload')).map((truth) => truth.upMbps / ceilingMbps(entry.up));
  const fits = (values: number[]) => !values.length
    || (Math.max(...values) >= THRESHOLDS.rate.min && median(values)! <= THRESHOLDS.rate.max);
  const ratios = [...down, ...up].map((ratio) => Number(ratio.toFixed(4)));
  return { ok: fits(down) && fits(up), ratios, detail: 'loss-free iperf3 goodput over the shaped payload ceiling' };
}

const rateReason = (rate: NonNullable<Validity['rate']>) => `rate: iperf3/ceiling ${rate.ratios.map((ratio) => ratio.toFixed(3)).join(', ')}`;

/** Results with each link's rate check recomputed by the current rule. */
export function revalidate(results: Results): Results {
  return {
    ...results,
    profiles: results.profiles.map((entry) => {
      const truths = entry.repeats.map(truthOf).filter((truth): truth is Truth => !!truth);
      const rate = rateCheck(entry.shape, truths);
      const reasons = [...entry.validity.reasons.filter((reason) => !reason.startsWith('rate:')), ...(rate && !rate.ok ? [rateReason(rate)] : [])];
      return { ...entry, validity: { ...entry.validity, rate, reasons, ok: reasons.length === 0 } };
    }),
  };
}

export function analyze(input: Results, findings: Finding[], expected: Profile[] = PROFILES): Analysis {
  const results = revalidate(input);
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
  const verdictGate = gate(final, findings);
  return {
    verdicts: final,
    ok: verdictGate.ok,
    gate: verdictGate,
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
      put(`${dir}.loaded.bias`, verdicts.find((item) => item.profile === entry.id && item.metric === `loaded.${dir}.bias`)?.error?.median);
      row[`${dir}.rough`] = String(weirs.filter((w) => (dir === 'download' ? w?.report?.download.rough : w?.report?.upload?.rough)).length);
      row[`${dir}.capped`] = String(weirs.filter((w) => (dir === 'download' ? w?.report?.download.capped : w?.report?.upload?.capped)).length);
    }
    put('dupDown', stat(weirs.map((w) => w?.duplicates?.down))?.median, 0);
    put('dupUp', stat(weirs.map((w) => w?.duplicates?.up))?.median, 0);
    put('idle.weir', stat(weirs.map((w) => w?.report?.latency?.medianMs))?.median);
    put('idle.truth', stat(truths.map((t) => t?.idleMs))?.median);
    put('data.mb', stat(weirs.map((w) => (w?.wire.ipBytes ?? NaN) / 1e6))?.max, 0);
    put('tail.mb', stat(weirs.map((w) => (typeof w?.wire.tailIpBytes === 'number' ? w.wire.tailIpBytes / 1e6 : null)))?.max);
    put('bytesUsed.mb',stat(weirs.map((w) => (w?.report?.bytesUsed ?? NaN) / 1e6))?.median, 0);
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
  const rate = rateCheck(entry, truths);
  if (rate && !rate.ok) reasons.push(rateReason(rate));
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
