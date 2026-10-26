import assert from 'node:assert/strict'
import { test } from 'node:test'
import { monthlyText, nextText } from '../../src/lib/schedule.ts'

const hour = 3_600_000
const noon = new Date(2026, 9, 26, 12, 0).getTime()

test('monthly data follows the schedule and the per-test estimate', () => {
  assert.equal(monthlyText('off', 250), null)
  assert.equal(monthlyText('every6Hours', 250), 'About 250 MB a test, 30 GB a month')
  assert.equal(monthlyText('daily', 250), 'About 250 MB a test, 7.5 GB a month')
  assert.equal(monthlyText('daily', 100), 'About 100 MB a test, 3 GB a month')
})

test('next test is due an interval after the last one', () => {
  assert.equal(nextText('off', noon, noon, false), null)
  assert.equal(nextText('every6Hours', null, noon, false), 'Next test in a few minutes')
  assert.equal(nextText('every6Hours', noon - 7 * hour, noon, false), 'Next test in a few minutes')
  assert.match(nextText('every6Hours', noon - hour, noon, false)!, /^Next test around 5:00/)
  assert.match(nextText('daily', noon - hour, noon, false)!, /^Next test tomorrow around 11:00/)
  assert.equal(nextText('daily', noon, noon, true), 'Paused while this connection is metered')
})
