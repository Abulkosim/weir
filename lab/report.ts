// Merges shard results, judges them and writes the docs.
//
// node lab/report.ts --in lab-out/shards --out lab-out/report   merge, judge, write
// node lab/report.ts --out lab-out/report --gate                 exit 1 unless the gate passes
// node lab/report.ts --docs                                      re-render docs/ from docs/accuracy.json

import { appendFileSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { analyze, type Finding } from './analyze.ts';
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
    shards: ordered.map((shard) => shard.result),
    profiles: ordered.flatMap((shard) => shard.profiles),
  };
}

function write(out: string, results: Results) {
  const findings = loadFindings();
  const analysis = analyze(slim(results), findings);
  const files = renderAll(slim(results), findings);
  mkdirSync(join(out, 'images'), { recursive: true });
  writeFileSync(join(out, 'results.json'), `${JSON.stringify(results)}\n`);
  for (const [name, text] of Object.entries(files)) writeFileSync(join(out, name), text);
  const summary = renderSummary(slim(results), analysis);
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
    const analysis = analyze(slim(JSON.parse(readFileSync(path, 'utf8')) as Results), loadFindings());
    const bad = analysis.verdicts.filter((item) => !['pass', 'xfail', 'xpass', 'report'].includes(item.status));
    for (const item of analysis.verdicts.filter((verdict) => verdict.status === 'xpass')) {
      console.log(`::warning title=Accuracy lab xpass::${item.profile} ${item.metric} passes but ${item.finding} expects it to fail`);
    }
    for (const item of bad) console.log(`::error title=Accuracy lab::${item.profile} ${item.metric} ${item.status}: ${item.detail}`);
    if (!analysis.ok) process.exit(1);
  }
}

if (process.argv[1]?.endsWith('lab/report.ts')) main();
