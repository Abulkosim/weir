import assert from 'node:assert/strict'
import { test } from 'node:test'
import { monthSummary, outageText } from '../../src/lib/month.ts'
import { demoReport } from '../../src/lib/demo.ts'
import type { Entry } from '../../src/lib/model.ts'

const minute = 60_000
const at = (month: number, day: number, down: number, up = 40, latency = 14): Entry => {
  const report = demoReport(0)
  report.startedAtMs = new Date(2026, month, day, 12).getTime()
  report.download = { ...report.download, mbps: down }
  report.upload = report.upload && { ...report.upload, mbps: up }
  report.latency = report.latency && { ...report.latency, medianMs: latency }
  return { kind: 'result', report }
}
const now = new Date(2026, 10, 20, 9).getTime()

test('medians of this month, compared with last month', () => {
  const entries: Entry[] = [
    at(10, 18, 300, 40, 14), at(10, 10, 280, 38, 16), at(10, 3, 150, 30, 30),
    { kind: 'failed', atMs: new Date(2026, 10, 5).getTime(), error: 'stalled' },
    at(9, 20, 250), at(9, 12, 270),
  ]
  const summary = monthSummary(entries, now)!
  assert.equal(summary.name, null)
  assert.equal(summary.tests, 3)
  assert.equal(summary.failed, 1)
  assert.equal(summary.download, 280)
  assert.equal(summary.upload, 38)
  assert.equal(summary.latency, 16)
  assert.equal(summary.before, 260)
})

test('outages count only their time inside the month', () => {
  const first = new Date(2026, 10, 1).getTime()
  const entries: Entry[] = [
    at(10, 18, 300),
    { kind: 'outage', startMs: new Date(2026, 10, 12, 14, 2).getTime(), endMs: new Date(2026, 10, 12, 14, 9).getTime() },
    { kind: 'outage', startMs: first - 10 * minute, endMs: first + 50 * minute },
    { kind: 'outage', startMs: first - 90 * minute, endMs: first - 30 * minute },
  ]
  const summary = monthSummary(entries, now)!
  assert.equal(summary.outages, 2)
  assert.equal(summary.outageMs, 57 * minute)
  assert.equal(outageText(summary, true), 'Offline 57 min · 2 outages')
  summary.outageMs = 125 * minute
  assert.equal(outageText(summary, true), 'Offline 2 h 5 min · 2 outages')
})

test('says whether no outages means none were seen', () => {
  const summary = monthSummary([at(10, 18, 300)], now)!
  assert.equal(outageText(summary, true), 'No outages seen')
  assert.equal(outageText(summary, false), 'Outage watch is off')
})

test('falls back to last month until this month has a test', () => {
  const summary = monthSummary([at(9, 20, 250), at(8, 2, 200)], now)!
  assert.equal(summary.name, new Date(2026, 9, 1).toLocaleDateString([], { month: 'long' }))
  assert.equal(summary.download, 250)
  assert.equal(summary.before, 200)
  assert.equal(monthSummary([at(8, 2, 200)], now), null)
  assert.equal(monthSummary([], now), null)
})

test('January compares with December of the year before', () => {
  const january = new Date(2027, 0, 15).getTime()
  const entries: Entry[] = [at(12, 10, 310), at(11, 10, 290)]
  const summary = monthSummary(entries, january)!
  assert.equal(summary.download, 310)
  assert.equal(summary.before, 290)
})
