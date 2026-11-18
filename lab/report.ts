// Merges shard results, judges them and writes the docs.
//
// node lab/report.ts --in lab-out/shards --out lab-out/report   merge, judge, write
// node lab/report.ts --out lab-out/report --gate                 exit 1 unless the gate passes
// node lab/report.ts --docs                                      re-render docs/ from docs/accuracy.json
//
// --profiles a,b, --repeats N (or LAB_PROFILES, LAB_REPEATS) and --shards 1,2 judge only what a
// filtered run planned; without them every link, shard and repeat must be there.

import { appendFileSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { analyze, gate, revalidate, type Analysis, type Finding, type Gate } from './analyze.ts';
import { selection, type Profile } from './profiles.ts';
import { renderAll, renderSummary } from './render.ts';
import { slim, type Results, type ShardFile } from './schema.ts';

const args = process.argv.slice(2);
const value = (flag: string) => (args.includes(flag) ? args[args.indexOf(flag) + 1] : undefined);

export function loadFindings(): Finding[] {
  return JSON.parse(readFileSync(new URL('./findings.json', import.meta.url), 'utf8')) as Finding[];
}

export function merge(shards: ShardFile[], meta: { commit: string; weirVersion: string; runUrl: string }): Results {
  const ordered = [...shards].sort((a, b) => a.shard - b.shard);
  const first = ordered[0];
  if (!first) throw new Error('no shard results');
  return {
    schema: 1,
    commit: meta.commit,
    weirVersion: meta.weirVersion,
    runUrl: meta.runUrl,
    startedAt: ordered.map((shard) => shard.startedAt).sort()[0]!,
    runner: first.runner,
    shards: ordered.map((shard) => ({ ...shard.result, runner: shard.result.runner ?? shard.runner })),
    profiles: ordered.flatMap((shard) => shard.profiles),
  };
}

function expected(): Profile[] {
  return selection({
    profiles: value('--profiles') ?? process.env.LAB_PROFILES,
    repeats: value('--repeats') ?? process.env.LAB_REPEATS,
    shards: value('--shards'),
  });
}

/** The run published in docs/accuracy.json, judged with today's findings: the previous run for the gate. */
function published(findings: Finding[]): Analysis | null {
  if (!existsSync('docs/accuracy.json')) return null;
  return analyze(JSON.parse(readFileSync('docs/accuracy.json', 'utf8')) as Results, findings);
}

function judge(results: Results): { analysis: Analysis; gate: Gate } {
  const findings = loadFindings();
  const analysis = analyze(slim(results), findings, expected());
  return { analysis, gate: gate(analysis.verdicts, findings, published(findings)?.verdicts ?? null) };
}

function write(out: string, merged: Results) {
  const findings = loadFindings();
  const judged = judge(merged);
  const results: Results = { ...merged, gate: { ok: judged.gate.ok, counts: judged.analysis.counts, errors: judged.gate.errors } };
  const files = renderAll(slim(results), findings, expected());
  mkdirSync(join(out, 'images'), { recursive: true });
  writeFileSync(join(out, 'results.json'), `${JSON.stringify(results)}\n`);
  for (const [name, text] of Object.entries(files)) writeFileSync(join(out, name), text);
  const summary = renderSummary(revalidate(slim(results)), judged.analysis, judged.gate);
  writeFileSync(join(out, 'summary.md'), summary);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, summary);
  console.log(summary);
}

function main() {
  const out = value('--out') ?? 'lab-out/report';
  if (args.includes('--docs')) {
    const results = JSON.parse(readFileSync('docs/accuracy.json', 'utf8')) as Results;
    for (const [name, text] of Object.entries(renderAll(results, loadFindings()))) writeFileSync(join('docs', name), text);
    return;
  }
  const input = value('--in');
  if (input) {
    const shards = readdirSync(input)
      .filter((name) => /^shard-\d+\.json$/.test(name))
      .map((name) => JSON.parse(readFileSync(join(input, name), 'utf8')) as ShardFile);
    const pkg = JSON.parse(readFileSync('package.json', 'utf8')) as { version: string };
    write(out, merge(shards, {
      commit: process.env.LAB_COMMIT ?? process.env.GITHUB_SHA ?? 'local',
      weirVersion: pkg.version,
      runUrl: process.env.LAB_RUN_URL ?? '',
    }));
  }
  if (args.includes('--gate')) {
    const path = join(out, 'results.json');
    if (!existsSync(path)) {
      console.error('No results to gate.');
      process.exit(1);
    }
    const { gate: verdict } = judge(JSON.parse(readFileSync(path, 'utf8')) as Results);
    for (const line of verdict.warnings) console.log(`::warning title=Accuracy lab::${line}`);
    for (const line of verdict.errors) console.log(`::error title=Accuracy lab::${line}`);
    if (!verdict.ok) process.exit(1);
  }
}

if (process.argv[1]?.endsWith('lab/report.ts')) {
  try {
    main();
  } catch (error) {
    console.error(String(error instanceof Error ? error.message : error));
    process.exit(2);
  }
}
