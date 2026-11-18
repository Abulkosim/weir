import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { test } from 'node:test'
import { analyze, applyFindings, fill, gate, placeholders, validity, withinBound, type Finding, type Verdict } from '../../lab/analyze.ts'
import { netemArgs, offloads, parseNstat, parseQdiscStats, parseSoftnet, reorderDelta, shape, tbfLimit, teardown, topologyUp, unshape } from '../../lab/net.ts'
import { cpuMax, cpuPeak, iperfBlock, parseIperf, parsePing, parseProcStat, wireMbps, type WireSample } from '../../lab/parse.ts'
import { ceilingMbps, expectedFar, FAR_MS, forShard, order, PROFILES, selection, SHARDS, streams, type Profile } from '../../lab/profiles.ts'
import { merge } from '../../lab/report.ts'
import { renderAll, renderMarkdown, renderSvg } from '../../lab/render.ts'
import { slim, type ProfileResult, type Repeat, type Results, type Truth, type Validity, type Weir } from '../../lab/schema.ts'
import { THRESHOLDS } from '../../lab/thresholds.ts'
import { PINNED_MEASUREMENTS } from '../../lab/twin/measurements.ts'

const fixture = (name: string) => readFileSync(new URL(`./fixtures/lab/${name}`, import.meta.url), 'utf8')
const section = (text: string, name: string) => text.split(`## ${name}\n`)[1]!.split('\n## ')[0]!
const findings: Finding[] = JSON.parse(readFileSync(new URL('../../lab/findings.json', import.meta.url), 'utf8'))
const gig5 = PROFILES.find(entry => entry.id === 'gig-5')!

test('the topology builds three namespaces and tears them down', () => {
  const up = topologyUp()
  assert.deepEqual(up.slice(0, 3), [['ip', 'netns', 'add', 'weir-c'], ['ip', 'netns', 'add', 'weir-r'], ['ip', 'netns', 'add', 'weir-s']])
  assert.ok(up.some(argv => argv.join(' ') === 'ip netns exec weir-r sysctl -q -w net.ipv4.ip_forward=1'))
  assert.ok(up.some(argv => argv.join(' ') === 'ip netns exec weir-c sysctl -q -w net.ipv6.conf.all.disable_ipv6=1'))
  assert.ok(up.some(argv => argv.join(' ') === 'ip netns exec weir-s sysctl -q -w net.ipv4.tcp_congestion_control=cubic'))
  assert.equal(teardown().length, 6)
  assert.deepEqual(teardown()[0], ['sh', '-c', 'ip netns pids weir-c 2>/dev/null | xargs -r kill -9'], 'stale lab processes die before the namespaces go')
  assert.deepEqual(teardown()[3], ['ip', 'netns', 'del', 'weir-c'])
  assert.deepEqual(offloads(false)[0], ['ip', 'netns', 'exec', 'weir-c', 'ethtool', '-K', 'c0', 'tso', 'off', 'gso', 'off', 'gro', 'off'])
})

test('shaping deletes before it adds, with delay before a tbf queue', () => {
  assert.deepEqual(unshape().map(argv => argv.join(' ')), [
    'ip netns exec weir-r tc qdisc del dev rc root',
    'ip netns exec weir-r tc qdisc del dev rs root',
  ])
  assert.deepEqual(shape(gig5, 'pareto').map(argv => argv.join(' ')), [
    'ip netns exec weir-r tc qdisc add dev rc root handle 1: netem limit 1000000 delay 2.5ms',
    'ip netns exec weir-r tc qdisc add dev rc parent 1:1 handle 10: tbf rate 1000mbit burst 125000 limit 1250000',
    'ip netns exec weir-r tc qdisc add dev rs root handle 1: netem limit 1000000 delay 2.5ms',
    'ip netns exec weir-r tc qdisc add dev rs parent 1:1 handle 10: tbf rate 1000mbit burst 125000 limit 1250000',
  ])
  const wifi = PROFILES.find(entry => entry.id === 'wifi-jitter')!
  assert.deepEqual(netemArgs(wifi, 'pareto'), ['netem', 'limit', '1000000', 'delay', '7.5ms', '3ms', 'distribution', 'paretonormal', 'rate', '10gbit', 'loss', 'random', '0.2%'])
  assert.deepEqual(netemArgs(wifi, 'slot'), ['netem', 'limit', '1000000', 'delay', '7.5ms', 'slot', '1ms', '6ms', 'loss', 'random', '0.2%'])
  assert.equal(tbfLimit(1, 1), 3028, 'never below two frames')
})

test('the matrix covers what the lab promises', () => {
  const rates = new Set(PROFILES.flatMap(entry => [entry.down, entry.up]))
  for (const rate of [5, 25, 100, 300, 1000]) assert.ok(rates.has(rate), `${rate} Mbps`)
  assert.equal(Math.min(...PROFILES.map(entry => entry.rttMs)), 5)
  assert.equal(Math.max(...PROFILES.map(entry => entry.rttMs)), 250)
  assert.equal(Math.max(...PROFILES.map(entry => entry.lossPct)), 2)
  assert.ok(PROFILES.some(entry => entry.down / entry.up >= 10), 'asymmetric')
  assert.equal(PROFILES.filter(entry => entry.jitter).length, 2)
  assert.equal(new Set(PROFILES.map(entry => entry.id)).size, PROFILES.length)
  for (const entry of PROFILES) {
    assert.ok(entry.repeats >= 3, entry.id)
    if (entry.lossPct > 0 || entry.jitter) assert.ok(entry.repeats >= 5, `${entry.id} is noisy`)
    assert.ok(entry.twinRepeats <= entry.repeats, entry.id)
    assert.ok(Math.abs(entry.rttMs - FAR_MS) >= 20, `${entry.id} sits too close to Weir's far rule`)
    if (entry.pair) assert.equal(PROFILES.find(other => other.id === entry.pair!.with)?.shard, entry.shard, 'pairs share a runner')
  }
  for (let shard = 1; shard <= SHARDS; shard++) assert.ok(forShard(shard).length > 0, `shard ${shard}`)
  assert.deepEqual(streams(expectedFar({ rttMs: 250 })), { down: 8, up: 6 })
  assert.deepEqual(streams(expectedFar({ rttMs: 60 })), { down: 6, up: 4 })
  assert.deepEqual(order(gig5, 0), ['truth', 'weir', 'twin'])
  assert.deepEqual(order(gig5, 1), ['weir', 'twin', 'truth'])
  assert.deepEqual(order(gig5, 2), ['twin', 'truth', 'weir'])
  const mobile = PROFILES.find(entry => entry.id === 'mobile')!
  assert.deepEqual(order(mobile, 4), ['weir', 'truth'], 'twin runs only its own repeats')
  assert.deepEqual(order(PROFILES.find(entry => entry.id === 'qreal-up')!, 0), ['weir'])
})

test('iperf3 goodput comes from receiver intervals and is checked against the end sum', () => {
  const down = parseIperf(fixture('iperf-down.json'))
  assert.equal(down.mbps, 960)
  assert.equal(down.mismatch, 0)
  assert.equal(down.streams, 6)
  assert.equal(down.retransmits, 12)
  const up = parseIperf(fixture('iperf-up.json'))
  assert.ok(Math.abs(up.mbps - 23.92) < 1e-9)
  assert.ok(up.mismatch > 0 && up.mismatch < 0.001)
  const unflagged = JSON.parse(fixture('iperf-up.json'))
  unflagged.server_output_json.intervals = [0, 1, 2, 3].map(start => ({ sum: { start, end: start + 1, seconds: 1, bytes: start < 2 ? 1_000_000 : 3_000_000, omitted: false } }))
  assert.equal(parseIperf(JSON.stringify(unflagged)).mbps, 24, 'an unflagged server omit period is dropped by time')
  const glitch = JSON.parse(fixture('iperf-up.json'))
  glitch.server_output_json.intervals = [
    { sum: { start: 0, end: 1, seconds: 1, bytes: 1_000_000, omitted: true } },
    { sum: { start: 1, end: 1, seconds: 2, bytes: 3_000_000, omitted: false } },
    ...[1, 2, 3].map(start => ({ sum: { start, end: start + 1, seconds: 1, bytes: 3_000_000, omitted: false } })),
  ]
  assert.equal(parseIperf(JSON.stringify(glitch)).mbps, 24, 'an omit-boundary interval with two seconds and one second of bytes is dropped')
  glitch.server_output_json.intervals.push({ sum: { start: 4, end: 4.5, seconds: 0.5, bytes: 0, omitted: false } })
  assert.equal(parseIperf(JSON.stringify(glitch)).mbps, 24, 'the short wait after the sender stops is dropped')
  assert.deepEqual([1, 3, 25, 1000].map(iperfBlock), [2048, 4096, 31744, 131072])
  assert.throws(() => parseIperf('{"error":"unable to connect to server 10.77.2.2"}'), /<addr>/)
  assert.throws(() => parseIperf('{"start":{"test_start":{"reverse":0}},"intervals":[]}'), /no receiver intervals/)
})

test('ping counts an unanswered probe as the 2000 ms timeout, like Weir', () => {
  const samples = parsePing(fixture('ping.txt'), 100)
  assert.deepEqual(samples.map(sample => Math.round(sample.rttMs)), [50, 52, 2000, 2000, 60])
  assert.deepEqual(samples.map(sample => Math.round(sample.sentMs - 1_700_000_000_000)), [0, 100, 200, 300, 2740])
})

test('tc, softnet, nstat and /proc/stat parsers read synthetic output', () => {
  const [netem, tbf] = parseQdiscStats(fixture('tc.json'))
  assert.deepEqual([netem!.kind, netem!.drops, netem!.packets], ['netem', 10, 1000])
  assert.deepEqual([tbf!.backlog, tbf!.qlen], [3028, 2])
  const proc = fixture('proc.txt')
  assert.deepEqual(parseSoftnet(section(proc, 'softnet_stat')), { processed: 0xa000 + 0xb000, dropped: 5, squeezed: 1 })
  const after = parseNstat(section(proc, 'nstat'))
  assert.equal(reorderDelta({}, after), 7)
  assert.equal(reorderDelta(after, after), 0)
  const busiest = cpuMax(parseProcStat(section(proc, 'stat-before')), parseProcStat(section(proc, 'stat-after')))
  assert.ok(Math.abs(busiest - 270 / 280) < 1e-9, 'one hot CPU shows, not the average')
})

test('wire goodput interpolates counters and removes headers', () => {
  const samples: WireSample[] = [
    { atMs: 0, rxBytes: 0, rxPackets: 0, txBytes: 0, txPackets: 0 },
    { atMs: 1000, rxBytes: 1514 * 1000, rxPackets: 1000, txBytes: 0, txPackets: 0 },
    { atMs: 2000, rxBytes: 1514 * 3000, rxPackets: 3000, txBytes: 0, txPackets: 0 },
  ]
  assert.ok(Math.abs(wireMbps(samples, 1000, 2000)! - (1448 * 2000 * 8) / 1e6) < 1e-9)
  assert.ok(Math.abs(wireMbps(samples, 500, 1500)! - (1448 * 1500 * 8) / 1e6) < 1e-9)
  assert.equal(wireMbps(samples, 1500, 2500), null, 'outside the samples')
})

const ok: Validity = {
  delay: { ok: true, medianMs: 5, p5: 5, p95: 5, detail: '' },
  rate: null, loss: null, drops: { ok: true, softnet: 0, txDropped: 0, detail: '' }, bloat: null, reorder: null, ok: true, reasons: [],
}

interface Numbers {
  down: number
  up: number
  wireDown?: number
  wireUp?: number
  truthDown?: number
  truthUp?: number
  idle?: number
  truthIdle?: number
  loaded?: number
  pingLoaded?: number
  ipMb?: number
  error?: string | null
}

function weir(numbers: Numbers): Weir {
  const speed = (mbps: number) => ({ mbps, p10: mbps, p90: mbps, varied: false, capped: false, rampedSlowly: false, rough: false, loadedMs: numbers.loaded ?? 20 })
  return {
    report: numbers.error === undefined || numbers.error === null
      ? { durationMs: 20_000, latency: { medianMs: numbers.idle ?? 5, jitterMs: 0.1, answered: 19, sent: 19 }, download: speed(numbers.down), upload: speed(numbers.up), uploadError: null, bytesUsed: 100e6 }
      : null,
    error: numbers.error ?? null,
    wallMs: 20_000,
    windows: { download: null, upload: null },
    wire: {
      downMbps: numbers.wireDown ?? numbers.down,
      upMbps: numbers.wireUp ?? numbers.up,
      idleMs: numbers.truthIdle ?? 5,
      loadedDownMs: numbers.pingLoaded ?? 20,
      loadedUpMs: numbers.pingLoaded ?? 20,
      ipBytes: (numbers.ipMb ?? 200) * 1e6,
      payloadBytes: 100e6,
      pingSent: 400,
      serverSpan: null,
    },
    serverBytes: { down: 0, up: 0, unknown: 0 },
    far: (numbers.idle ?? 5) > 100,
    cpuMax: 0.4,
    saturated: false,
  }
}

function truth(numbers: Numbers): Truth {
  return {
    downMbps: numbers.truthDown ?? numbers.down, upMbps: numbers.truthUp ?? numbers.up, idleMs: numbers.truthIdle ?? 5, jitterMs: 0.1,
    loadedDownMs: 20, loadedUpMs: 20, streams: { down: 6, up: 4 }, mismatch: { down: 0, up: 0 }, cpuMax: 0.3, saturated: false,
  }
}

function result(definition: Profile, runs: Numbers[], validityOverride: Validity = ok): ProfileResult {
  const repeats: Repeat[] = runs.map((numbers, index) => ({
    order: order(definition, index),
    wallMs: 60_000,
    truth: definition.truth ? truth(numbers) : null,
    weir: weir(numbers),
    twin: index < definition.twinRepeats
      ? { down: numbers.down, up: numbers.up, idle: 5, jitter: 0.1, downLoaded: 20, upLoaded: 20, durationMs: 30_000, errors: [], blocked: 0, cpuMax: 0.3, saturated: false }
      : null,
  }))
  return { id: definition.id, shape: { ...definition }, shard: definition.shard, planned: runs.length, offloads: 'off', validity: validityOverride, repeats }
}

function goodRuns(definition: Profile): Numbers[] {
  const down = ceilingMbps(definition.down) * 0.97
  const up = ceilingMbps(definition.up) * 0.97
  return Array.from({ length: definition.repeats }, () => ({ down, up, idle: definition.rttMs, truthIdle: definition.rttMs, loaded: definition.rttMs + 10, pingLoaded: definition.rttMs + 10 }))
}

function results(profiles: ProfileResult[]): Results {
  return {
    schema: 1, commit: 'abc123def4567890', weirVersion: '0.7.3', runUrl: 'https://github.com/Abulkosim/weir/actions/runs/1', startedAt: '2026-11-17T20:00:00.000Z',
    runner: { kernel: 'test', cpuModel: 'test CPU', cpus: 4, iperf3: '3.16', chromium: '1', speedtestLib: '1.16.0', tcpCc: 'cubic', sysctls: {}, offloads: 'off', jitterMode: 'pareto', netdevMaxBacklog: 10000, weirProfile: 'lab' },
    shards: Array.from({ length: SHARDS }, (_, index) => ({
      shard: index + 1, wallMs: 1_800_000, cpuModel: 'test CPU', kernel: 'test',
      baseline: { iperfDown: 15000, iperfUp: 15000, weirDown: 4000, weirUp: 3000, weirError: null, twinDown: 3000, twinUp: 2000, serverCpuCores: 1, serverGbps: 7, softnetDrops: 0, weirLoadedInflationMs: 0.3 },
    })),
    profiles,
  }
}

function allGood(): ProfileResult[] {
  return PROFILES.map(definition => {
    const runs = goodRuns(definition)
    if (definition.pair?.role === 'bloat') runs.forEach(run => Object.assign(run, { loaded: 500, pingLoaded: 520 }))
    return result(definition, runs)
  })
}

const pick = (verdicts: Verdict[], profile: string, metric: string) => verdicts.find(item => item.profile === profile && item.metric === metric)!

test('a lab where Weir matches every truth passes the gate', () => {
  const analysis = analyze(results(allGood()), [])
  const bad = analysis.verdicts.filter(item => !['pass', 'report'].includes(item.status))
  assert.deepEqual(bad.map(item => `${item.profile} ${item.metric} ${item.status} ${item.detail}`), [])
  assert.ok(analysis.ok)
  assert.equal(pick(analysis.verdicts, 'qreal-up', 'idle').status, 'report', 'Q-real is never gated')
})

test('Weir off the wire in its own window is weir-math; on the wire but under iperf3 is weir-load', () => {
  const profiles = allGood()
  const gig = profiles.find(entry => entry.id === 'gig-5')!
  const definition = gig5
  const truthDown = ceilingMbps(definition.down) * 0.97
  Object.assign(gig, result(definition, Array.from({ length: 3 }, () => ({ down: truthDown * 0.8, up: truthDown, wireDown: truthDown * 0.95, idle: 5, truthIdle: 5, truthDown }))))
  let verdicts = analyze(results(profiles), []).verdicts
  assert.equal(pick(verdicts, 'gig-5', 'download.wire').suspect, 'weir-math')
  assert.equal(pick(verdicts, 'gig-5', 'download.capacity').suspect, 'weir-math')

  Object.assign(gig, result(definition, Array.from({ length: 3 }, () => ({ down: truthDown * 0.8, up: truthDown, wireDown: truthDown * 0.8, idle: 5, truthIdle: 5, truthDown }))))
  verdicts = analyze(results(profiles), []).verdicts
  assert.equal(pick(verdicts, 'gig-5', 'download.wire').status, 'pass')
  assert.equal(pick(verdicts, 'gig-5', 'download.capacity').status, 'fail')
  assert.equal(pick(verdicts, 'gig-5', 'download.capacity').suspect, 'weir-load')
})

test('physics, data ceiling, latency and engine errors are judged', () => {
  const profiles = allGood()
  const fiber = PROFILES.find(entry => entry.id === 'fiber-300')!
  const index = profiles.findIndex(entry => entry.id === 'fiber-300')
  profiles[index] = result(fiber, [
    { down: 320, up: 280, truthDown: 320, truthUp: 280, idle: 10, truthIdle: 10, ipMb: 650 },
    { down: 280, up: 0, error: 'tooShort', truthDown: 280, truthUp: 280 },
    { down: 280, up: 280, truthDown: 280, idle: 14, truthIdle: 10, loaded: 80, pingLoaded: 40 },
  ])
  const verdicts = analyze(results(profiles), []).verdicts
  assert.equal(pick(verdicts, 'fiber-300', 'download.physics').status, 'fail')
  assert.equal(pick(verdicts, 'fiber-300', 'data.ceiling').status, 'fail')
  assert.equal(pick(verdicts, 'fiber-300', 'idle').status, 'fail')
  assert.equal(pick(verdicts, 'fiber-300', 'loaded.download').status, 'fail')
  const engine = pick(verdicts, 'fiber-300', 'validity')
  assert.equal(engine.status, 'fail')
  assert.equal(engine.suspect, 'weir-math')
  assert.match(engine.detail, /tooShort ×1/)
})

test('a noisy truth is inconclusive and bad input marks the link invalid', () => {
  const profiles = allGood()
  const lossy = PROFILES.find(entry => entry.id === 'lossy-1')!
  const index = profiles.findIndex(entry => entry.id === 'lossy-1')
  profiles[index] = result(lossy, [10, 40, 15, 60, 30].map(truthDown => ({ down: truthDown * 0.5, up: 18, truthDown, idle: 40, truthIdle: 40 })))
  let verdicts = analyze(results(profiles), []).verdicts
  assert.equal(pick(verdicts, 'lossy-1', 'download.capacity').status, 'inconclusive')

  profiles[index] = result(lossy, goodRuns(lossy), { ...ok, ok: false, reasons: ['loss: 0.2 % for 1 %'] })
  verdicts = analyze(results(profiles), []).verdicts
  assert.equal(pick(verdicts, 'lossy-1', 'download.wire').status, 'invalid-input')
  assert.equal(pick(verdicts, 'lossy-1', 'download.wire').suspect, 'input')
  assert.equal(pick(verdicts, 'lossy-1', 'twin.download').status, 'report')
})

test('Weir must show bufferbloat that is physically there', () => {
  const profiles = allGood()
  const deep = profiles.find(entry => entry.id === 'dsl-25-bloat')!
  for (const repeat of deep.repeats) (repeat.weir as Weir).report!.download.loadedMs = 120
  for (const repeat of deep.repeats) (repeat.weir as Weir).report!.upload!.loadedMs = 120
  const verdict = pick(analyze(results(profiles), []).verdicts, 'dsl-25-bloat', 'bloat')
  assert.equal(verdict.status, 'fail')
  assert.match(verdict.detail, /concurrent ping 490 ms/)
})

const bare = (profile: string, metric: Verdict['metric'], status: Verdict['status'], extra: Partial<Verdict> = {}): Verdict =>
  ({ profile, metric, status, suspect: null, finding: null, detail: 'd', weir: null, truth: null, error: null, ...extra })
const range = (min: number, max: number) => ({ median: (min + max) / 2, min, max, n: 3 })

test('open findings turn failures into expected failures only within their bound', () => {
  const finding: Finding = {
    id: 'F-1', title: 't', status: 'open', where: 'x.rs:1', cause: 'c',
    expect: [{ profile: 'gig-5', metric: 'upload.wire', min: -5, max: 10, seen: '2 of 4 runs' }, { profile: 'gig-5', metric: 'idle', min: -3, max: 3, seen: '1 of 4 runs' }],
  }
  const verdicts = applyFindings([
    bare('gig-5', 'upload.wire', 'fail', { error: range(1, 8) }),
    bare('gig-5', 'idle', 'pass', { error: range(-1, 1) }),
    bare('gig-60', 'idle', 'fail', { error: range(-9, 9) }),
  ], [finding])
  assert.deepEqual(verdicts.map(item => item.status), ['xfail', 'xpass', 'fail'])
  const worse = applyFindings([bare('gig-5', 'upload.wire', 'fail', { error: range(1, 40) })], [finding])[0]!
  assert.equal(worse.status, 'fail', 'an upload 40 % over the wire is a new failure, not the finding')
  assert.match(worse.detail, /outside F-1's expected failure \(every per-repeat error within -5 to \+10\)/)
  assert.equal(applyFindings([bare('gig-5', 'upload.wire', 'fail', { error: range(1, 8) })], [{ ...finding, status: 'fixed' }])[0]!.status, 'fail')
})

test('a validity, missing or gap expectation covers only the failure it names', () => {
  const kinds = { profile: 'gig-5', metric: 'validity' as const, errors: ['upload tooShort'], seen: '4 of 4 runs' }
  assert.ok(withinBound(bare('gig-5', 'validity', 'fail', { kinds: ['upload tooShort'] }), kinds))
  assert.ok(!withinBound(bare('gig-5', 'validity', 'fail', { kinds: ['download stalled', 'upload tooShort'] }), kinds), 'a new download crash is not F-1')
  const missing = { profile: 'gig-5', metric: 'upload.wire' as const, missing: true, seen: '4 of 4 runs' }
  assert.ok(withinBound(bare('gig-5', 'upload.wire', 'fail'), missing))
  assert.ok(!withinBound(bare('gig-5', 'upload.wire', 'fail', { weir: range(900, 950), error: range(30, 40) }), missing), 'once uploads finish, their numbers are judged')
  const gaps = { profile: 'geo-250', metric: 'loaded.download' as const, gaps: 1, seen: '1 of 4 runs' }
  assert.ok(withinBound(bare('geo-250', 'loaded.download', 'fail', { gaps: { count: 1, status: 'pass' } }), gaps))
  assert.ok(!withinBound(bare('geo-250', 'loaded.download', 'fail', { gaps: { count: 2, status: 'pass' } }), gaps))
  assert.ok(!withinBound(bare('geo-250', 'loaded.download', 'fail', { gaps: { count: 1, status: 'fail' } }), gaps))
  assert.ok(!withinBound(bare('geo-250', 'loaded.download', 'fail', { error: range(-90, 0) }), gaps))
})

test('committed findings are bounded, narrow and still catch the regressions they would hide', () => {
  for (const finding of findings) {
    for (const pair of finding.expect) {
      const kinds = [pair.errors ? 1 : 0, pair.missing ? 1 : 0, pair.min !== undefined || pair.max !== undefined || pair.gaps ? 1 : 0].reduce((a, b) => a + b, 0)
      assert.equal(kinds, 1, `${finding.id} ${pair.profile} ${pair.metric} has exactly one kind of bound`)
      assert.match(pair.seen, /^\d of \d runs$/, `${finding.id} ${pair.profile} ${pair.metric} says how often it failed`)
    }
  }
  const physics = findings.flatMap(finding => finding.expect.filter(pair => pair.metric === 'upload.physics' && !pair.missing))
  assert.ok(physics.length <= 4, 'upload.physics stays gated on most links')
  const profiles = allGood()
  const cable = PROFILES.find(entry => entry.id === 'cable-100-10-cf')!
  profiles[profiles.findIndex(entry => entry.id === cable.id)] = result(cable, goodRuns(cable).map(run => ({ ...run, up: run.up * 2, wireUp: run.up, truthUp: run.up })))
  const doubled = analyze(results(profiles), findings).verdicts
  assert.equal(pick(doubled, cable.id, 'upload.physics').status, 'fail', 'dropping the server-time subtraction fails the gate')
  assert.equal(pick(doubled, cable.id, 'upload.wire').status, 'fail')
})

test('the gate warns on one unjudged link, fails on two, and closes findings that keep passing', () => {
  const one = [bare('lossy-1', 'download.capacity', 'inconclusive'), bare('lossy-1', 'upload.capacity', 'invalid-input')]
  assert.equal(gate(one, []).ok, true)
  assert.equal(gate(one, []).warnings.length, 2)
  const two = [...one, bare('mobile', 'idle', 'inconclusive')]
  assert.equal(gate(two, []).ok, false)
  assert.equal(gate([bare('gig-5', 'idle', 'fail')], []).ok, false)
  const finding: Finding = { id: 'F-9', title: 't', status: 'open', where: 'x.rs:1', cause: 'c', expect: [{ profile: 'gig-5', metric: 'idle', min: -9, max: 9, seen: '1 of 4 runs' }] }
  const passing = [bare('gig-5', 'idle', 'xpass', { finding: 'F-9' })]
  assert.equal(gate(passing, [finding]).ok, true, 'one passing run warns')
  assert.equal(gate(passing, [finding], [bare('gig-5', 'idle', 'xfail')]).ok, true)
  const closed = gate(passing, [finding], passing)
  assert.equal(closed.ok, false, 'two runs in a row: close or narrow it')
  assert.match(closed.errors[0]!, /^F-9: /)
})

test('a finished run without a Weir number fails the check instead of being skipped', () => {
  const profiles = allGood()
  const deep = profiles.find(entry => entry.id === 'dsl-25-bloat')!
  ;(deep.repeats[0]!.weir as Weir).report!.download.loadedMs = null
  ;(deep.repeats[1]!.weir as Weir).report!.download.loadedMs = null
  const verdicts = analyze(results(profiles), []).verdicts
  const loaded = pick(verdicts, 'dsl-25-bloat', 'loaded.download')
  assert.equal(loaded.status, 'fail')
  assert.match(loaded.detail, /no download loaded latency in 2 of 3 finished runs/)
  assert.deepEqual(loaded.gaps, { count: 2, status: 'pass' })
  const lost = allGood()
  for (const repeat of lost.find(entry => entry.id === 'gig-5')!.repeats.slice(0, 2)) (repeat.weir as Weir).wire.loadedDownMs = null
  assert.equal(pick(analyze(results(lost), []).verdicts, 'gig-5', 'loaded.download').status, 'inconclusive', 'two runs without a ping is the lab, not Weir')
})

test('idle latency is judged against the ping of the same run, every repeat', () => {
  const profiles = allGood()
  const mobile = PROFILES.find(entry => entry.id === 'mobile')!
  const runs = goodRuns(mobile).map(run => ({ ...run, idle: 48, truthIdle: 48 }))
  const entry = result(mobile, runs)
  for (const repeat of entry.repeats) (repeat.truth as Truth).idleMs = 53
  profiles[profiles.findIndex(item => item.id === 'mobile')] = entry
  let idle = pick(analyze(results(profiles), []).verdicts, 'mobile', 'idle')
  assert.equal(idle.status, 'pass', 'a truth-step ping minutes away is not the reference')
  assert.equal(idle.truth!.median, 48)
  ;(entry.repeats[0]!.weir as Weir).report!.latency!.medianMs = 60
  idle = pick(analyze(results(profiles), []).verdicts, 'mobile', 'idle')
  assert.equal(idle.status, 'fail', 'one repeat off is a failure even on a jittery link')
})

test('loaded latency that is low in every run fails the bias check', () => {
  const profiles = allGood()
  const deep = profiles.find(entry => entry.id === 'dsl-25-bloat')!
  for (const repeat of deep.repeats) (repeat.weir as Weir).report!.download.loadedMs = 520 - 50
  const verdicts = analyze(results(profiles), []).verdicts
  assert.equal(pick(verdicts, 'dsl-25-bloat', 'loaded.download').status, 'pass', 'inside the per-run band')
  const bias = pick(verdicts, 'dsl-25-bloat', 'loaded.download.bias')
  assert.equal(bias.status, 'fail')
  assert.equal(bias.error!.median, -50)
})

test('the CPU guard sees a core pinned for one second inside a longer idle step', () => {
  const at = (busy: number, total: number) => [{ busy, total }, { busy: 0, total }]
  const samples = [at(0, 0), at(5, 25), at(30, 50), at(55, 75), at(80, 100), at(105, 125), at(105, 600)]
  assert.ok(cpuMax(samples[0]!, samples[samples.length - 1]!) < 0.2, 'the step average hides it')
  assert.equal(cpuPeak(samples, 4), 1)
  assert.equal(cpuPeak(samples.slice(0, 1), 4), 0)
})

test('a filtered run is checked against what it planned, and bad inputs stop it', () => {
  assert.equal(selection().length, PROFILES.length)
  const two = selection({ profiles: 'gig-5, mobile', repeats: '2' })
  assert.deepEqual(two.map(entry => [entry.id, entry.repeats, entry.twinRepeats]), [['gig-5', 2, 2], ['mobile', 2, 2]])
  assert.deepEqual(selection({ shards: '1' }).map(entry => entry.id), forShard(1).map(entry => entry.id))
  assert.throws(() => selection({ profiles: 'gig5' }), /unknown link gig5/)
  assert.throws(() => selection({ repeats: 'three' }), /positive whole number/)
  assert.throws(() => selection({ repeats: '0' }), /positive whole number/)
  assert.throws(() => selection({ shards: '6' }), /shard must be 1 to 5/)
  const all = results(allGood().filter(entry => entry.id === 'gig-5' || entry.id === 'mobile').map(entry => ({ ...entry, repeats: entry.repeats.slice(0, 2) })))
  const verdicts = analyze(all, [], two).verdicts
  assert.deepEqual(verdicts.filter(item => item.metric === 'coverage').map(item => item.profile), [], 'nothing outside the selection is missing')
})

test('each shard keeps its own runner', () => {
  const base = results([])
  const shard = (number: number, cpuModel: string) => ({
    schema: 1 as const, shard: number, runner: { ...base.runner, cpuModel }, startedAt: `2026-11-17T20:0${number}:00.000Z`,
    result: { ...base.shards[number - 1]!, cpuModel }, profiles: [],
  })
  const merged = merge([shard(2, 'AMD EPYC 7763'), shard(1, 'Intel Xeon')], { commit: 'c', weirVersion: '0', runUrl: '' })
  assert.deepEqual(merged.shards.map(item => item.runner?.cpuModel), ['Intel Xeon', 'AMD EPYC 7763'])
  const markdown = renderMarkdown(merged, [])
  assert.match(markdown, /ran on 2 different CPU models/)
})

test('missing shards, links and repeats fail the gate', () => {
  const profiles = allGood().filter(entry => entry.id !== 'geo-250')
  profiles.find(entry => entry.id === 'gig-5')!.repeats.pop()
  const all = results(profiles)
  all.shards.pop()
  const analysis = analyze(all, [])
  assert.equal(analysis.ok, false)
  assert.equal(pick(analysis.verdicts, 'geo-250', 'coverage').status, 'fail')
  assert.match(pick(analysis.verdicts, 'gig-5', 'coverage').detail, /2 of 3/)
  assert.equal(pick(analysis.verdicts, 'baseline-5', 'coverage').status, 'fail')
})

test('the baseline proves the lab can carry the fastest link', () => {
  const all = results(allGood())
  all.shards[0]!.baseline.iperfDown = 900
  all.shards[1]!.baseline.softnetDrops = 4
  all.shards[2]!.baseline.serverCpuCores = 4
  const verdicts = analyze(all, []).verdicts.filter(item => item.metric === 'baseline' && item.status === 'fail')
  assert.deepEqual(verdicts.map(item => item.profile), ['baseline-1.iperf3', 'baseline-2.softnet', 'baseline-3.server'])
})

test('lab input checks catch a wrong delay, rate, loss, drops and buffer', () => {
  const deep = PROFILES.find(entry => entry.id === 'dsl-25-bloat')!
  const loaded = { ...truth({ down: 23.9, up: 4.78 }), loadedDownMs: 500, loadedUpMs: 520 }
  assert.equal(validity(deep, Array(40).fill(30.05), [loaded], { down: null, up: null }, 0, 0, 0).ok, true)
  const shallow = validity(deep, Array(40).fill(30.05), [{ ...loaded, loadedDownMs: 60, loadedUpMs: 60 }], { down: null, up: null }, 0, 0, 0)
  assert.deepEqual(shallow.reasons, ['bloat: iperf3-loaded 60.0 ms'])
  const wrong = validity(deep, Array(40).fill(33), [{ ...loaded, downMbps: 20 }], { down: null, up: null }, 3, 1, 0)
  assert.equal(wrong.delay.ok, false)
  assert.equal(wrong.rate!.ok, false)
  assert.equal(wrong.drops.ok, false)
  const lossy = PROFILES.find(entry => entry.id === 'lossy-1')!
  assert.equal(validity(lossy, Array(40).fill(40), [], { down: 0.0105, up: 0.009 }, 0, 0, 0).ok, true)
  assert.equal(validity(lossy, Array(40).fill(40), [], { down: 0.004, up: 0.009 }, 0, 0, 0).loss!.ok, false)
  const wifi = PROFILES.find(entry => entry.id === 'wifi-jitter')!
  const spread = Array.from({ length: 40 }, (_, index) => 11 + (index % 10))
  assert.equal(validity(wifi, spread, [], { down: 0.002, up: 0.002 }, 0, 0, 0).delay.ok, true)
  assert.equal(validity(wifi, Array(40).fill(15), [], { down: 0.002, up: 0.002 }, 0, 0, 0).delay.ok, false, 'jitter must be there')
})

test('every threshold carries a reason', () => {
  for (const [name, limits] of Object.entries(THRESHOLDS)) {
    assert.ok(typeof limits.reason === 'string' && limits.reason.length > 40, name)
    assert.ok(Object.keys(limits).length > 1, `${name} has a limit`)
  }
})

test('findings cite numbers only through placeholders', () => {
  const typed = /\d+(\.\d+)?\s*(Mbps|ms|%|MB|Gbps)/
  for (const finding of findings) {
    assert.match(finding.id, /^F-/)
    assert.match(finding.where, /:\d+/)
    assert.ok(!typed.test(finding.cause), `${finding.id} cause hand-types a number`)
    assert.ok(!typed.test(finding.title), `${finding.id} title hand-types a number`)
  }
  assert.equal(fill('{gig-5.upload.weir} and {nope.x}', { 'gig-5': { 'upload.weir': '940.1' } }), '940.1 and {?nope.x}')
  assert.deepEqual(placeholders('{lossy-0.1.download.err}'), [{ profile: 'lossy-0.1', key: 'download.err' }])
})

test('the twin never asks for packet loss or RPKI checks', () => {
  const types = new Set(PINNED_MEASUREMENTS.map(entry => entry.type))
  assert.deepEqual([...types].sort(), ['download', 'latency', 'upload'])
})

test('rendering is deterministic and covers every link', () => {
  const all = results(allGood())
  const markdown = renderMarkdown(all, findings)
  assert.equal(markdown, renderMarkdown(all, findings))
  for (const entry of PROFILES) assert.ok(markdown.includes(`### ${entry.id}\n`), entry.id)
  assert.ok(markdown.includes('Gate at run time: not recorded. Re-judged with the findings and rules in this commit: **passing**'))
  assert.ok(renderMarkdown({ ...all, gate: { ok: false, counts: {}, errors: ['x'] } }, findings).includes('Gate at run time: **failing**. Re-judged with the findings and rules in this commit: **passing** (they differ: see Findings)'))
  assert.ok(!/\{\?/.test(markdown), 'every placeholder resolves')
  const analysis = analyze(all, findings)
  const light = renderSvg(all, analysis, 'light')
  const dark = renderSvg(all, analysis, 'dark')
  assert.ok(light.startsWith('<svg') && dark.startsWith('<svg'))
  assert.notEqual(light, dark)
  assert.ok(!/10\.77\./.test(markdown + light), 'no lab addresses in the docs')
})

test('slim results drop traces and request logs', () => {
  const all = results(allGood())
  const repeat = all.profiles[0]!.repeats[0]!
  Object.assign(repeat.weir, { trace: [{}], uploadRequests: [] })
  const slimmed = slim(all)
  assert.equal('trace' in slimmed.profiles[0]!.repeats[0]!.weir, false)
  assert.equal('trace' in all.profiles[0]!.repeats[0]!.weir, true)
})

test('committed accuracy docs are regenerated from docs/accuracy.json', () => {
  const root = new URL('../../docs/', import.meta.url)
  const files = ['accuracy.json', 'accuracy.md', 'images/accuracy-light.svg', 'images/accuracy-dark.svg']
  const present = files.filter(name => existsSync(new URL(name, root)))
  if (!present.length) return
  assert.deepEqual(present, files, 'all generated docs or none')
  const committed = JSON.parse(readFileSync(new URL('accuracy.json', root), 'utf8')) as Results
  const rendered = renderAll(committed, findings)
  for (const name of files) assert.equal(readFileSync(new URL(name, root), 'utf8'), rendered[name], `${name} is stale: run node lab/report.ts --docs`)
  for (const finding of findings) {
    for (const placeholder of placeholders(finding.cause)) {
      assert.ok(analyze(committed, findings).facts[placeholder.profile]?.[placeholder.key] !== undefined, `${finding.id} cites {${placeholder.profile}.${placeholder.key}}`)
    }
  }
})
