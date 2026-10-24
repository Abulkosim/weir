import assert from 'node:assert/strict'
import { test } from 'node:test'
import { slowHours } from '../../src/lib/hours.ts'
import { demoReport } from '../../src/lib/demo.ts'
import type { Entry } from '../../src/lib/model.ts'

const hour = 3_600_000
const at = (h: number, down: number, day = 0): Entry => {
  const report = demoReport(0)
  report.startedAtMs = day * 24 * hour + h * hour
  report.download = { ...report.download, mbps: down }
  return { kind: 'result', report }
}
const utcHour = (ms: number) => new Date(ms).getUTCHours()

test('too little history shows nothing', () => {
  assert.equal(slowHours([at(9, 300), at(10, 300), at(11, 300)], utcHour), null)
  const sameHour = Array.from({ length: 12 }, (_, day) => at(9, 300, day))
  assert.equal(slowHours(sameHour, utcHour), null, 'needs three different hours')
})

test('names the slowest run of evening hours', () => {
  const entries: Entry[] = []
  for (let day = 0; day < 4; day++) {
    entries.push(at(9, 310, day), at(13, 300, day), at(20, 150, day), at(21, 140, day))
  }
  entries.push({ kind: 'failed', atMs: 20 * hour, error: 'stalled' })
  const result = slowHours(entries, utcHour)!
  assert.deepEqual(result.slow, { from: 20, to: 22, median: 145 })
  assert.equal(result.buckets[20]!.count, 4)
  assert.equal(result.buckets[3]!.median, null)
  assert.equal(result.usual, 225)
})

test('a steady line has no slow hours', () => {
  const entries = Array.from({ length: 12 }, (_, index) => at(8 + index, 300 + (index % 3) * 5))
  assert.equal(slowHours(entries, utcHour)!.slow, null)
})
